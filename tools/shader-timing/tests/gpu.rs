//! The harness on Mesa's software renderer, llvmpipe: every plan compiles
//! and draws, a draw writes what its shader computes, and both clocks wait
//! for the GPU to finish a frame.

use vtium_shader_timing::device::{Choice, Device};
use vtium_shader_timing::source::{self, Mode, Shader};
use vtium_shader_timing::timing::{Bench, Pass, Surface, terminal_frame};
use vtium_shader_timing::{plans, summarize, time_plan};

fn software() -> Device {
    Device::open(&Choice::Software).expect("Mesa's software renderer is needed for these tests")
}

/// A shader that runs `steps` rounds of a hash for each pixel, seeded by
/// the pixel it reads, so the compiler can neither drop nor hoist them.
fn busy(steps: u32) -> Shader {
    let text = format!(
        "// timing: reads 1\n// timing: work {steps} rounds of a hash\n// timing: origin the harness's tests\n\
         void mainImage(out vec4 o, in vec2 p) {{\n\
             vec4 c = texture(iChannel0, p / iResolution.xy);\n\
             float x = c.r + p.x * 0.001;\n\
             for (int i = 0; i < {steps}; i++) {{ x = fract(sin(x * 12.9898 + float(i)) * 43758.5453); }}\n\
             o = vec4(x, c.gb, 1.0);\n\
         }}\n"
    );
    source::parse("busy", Box::leak(text.into_boxed_str())).unwrap()
}

#[test]
fn every_plan_compiles_and_draws_in_every_mode() {
    let device = software();
    let bench = Bench::new(&device, 64, 40).unwrap();
    let shaders = source::catalog().unwrap();
    for plan in plans(&shaders) {
        for mode in Mode::ALL {
            let row = time_plan(&bench, &plan, mode, 0, 1, 60.0)
                .unwrap_or_else(|e| panic!("{} ({}): {e}", plan.label(), mode.name()));
            assert_eq!(row.frames, 1);
            assert!(row.wall.0 > 0.0);
        }
    }
}

#[test]
fn negative_draws_the_inverse_of_the_frame_upright() {
    let device = software();
    let (w, h) = (64, 40);
    let bench = Bench::new(&device, w, h).unwrap();
    let shaders = source::catalog().unwrap();
    let negative = shaders.iter().find(|s| s.name == "negative").unwrap();
    let frame = terminal_frame(w, h);
    for mode in [Mode::Plain, Mode::Smooth, Mode::GlLinear] {
        let program = bench
            .program(&source::single(negative, mode).unwrap())
            .unwrap();
        let pass = Pass {
            program: &program,
            input: Surface::Frame,
            output: Surface::Frame,
            linear: mode.linear_sampler(),
        };
        bench.run(&[pass], 0, 1, 60.0).unwrap();
        // GL reads rows from the bottom, and the frame's first row is the
        // picture's top
        for (x, y) in [(0, 0), (13, 7), (31, 22), (63, 39), (5, 33)] {
            let i = ((h - 1 - y) * w + x) as usize * 4;
            let want = [255 - frame[i], 255 - frame[i + 1], 255 - frame[i + 2], 255];
            assert_eq!(
                bench.pixel(x, y).unwrap(),
                want,
                "{} at {x},{y}",
                mode.name()
            );
        }
        bench.delete(program);
    }
}

#[test]
fn both_clocks_wait_for_the_gpu_to_finish_the_frame() {
    let device = software();
    let bench = Bench::new(&device, 256, 256).unwrap();
    let time = |steps| {
        let shader = busy(steps);
        let program = bench
            .program(&source::single(&shader, Mode::Plain).unwrap())
            .unwrap();
        let pass = Pass {
            program: &program,
            input: Surface::Frame,
            output: Surface::Frame,
            linear: false,
        };
        let samples = bench.run(&[pass], 1, 5, 60.0).unwrap();
        bench.delete(program);
        summarize(&samples)
    };
    let (gpu_light, wall_light) = time(40);
    let (gpu_heavy, wall_heavy) = time(640);
    // sixteen times the work takes well over four times as long, unless a
    // clock stops before the GPU has done it
    assert!(
        wall_heavy.0 > 4.0 * wall_light.0,
        "the CPU's clock reads {} ms for 640 rounds and {} ms for 40",
        wall_heavy.0,
        wall_light.0
    );
    if bench.timer_queries() {
        let (light, heavy) = (gpu_light.unwrap().0, gpu_heavy.unwrap().0);
        // llvmpipe's query brackets the draws faithfully only when each of
        // llvmpipe's draw threads has a CPU of its own. Then the GPU's
        // clock also reads well over four times as long for sixteen times
        // the work, and a query that ends before the draws reads about the
        // same for both. With fewer CPUs the ratio says nothing either way:
        // a query ended before the draws passed 20 of 20 runs on 4 CPUs
        // with 32 draw threads, and 10 of 10 on 16 CPUs held by taskset,
        // and the correct code failed 10 of 60 runs on 2 CPUs and 1 of 60
        // on 4. On 32 CPUs the ended-early query failed 20 of 20 runs and
        // the correct code passed 20 of 20. So the test holds the GPU's
        // clock to the ratio only on a host with at least 32 CPUs, which
        // is where `make gates` runs, and leaves it out on CI's runners.
        let cpus = std::thread::available_parallelism().map_or(1, |n| n.get());
        if cpus >= 32 {
            assert!(
                heavy > 4.0 * light,
                "the GPU's clock reads {heavy} ms for 640 rounds and {light} ms for 40"
            );
        }
        // The query lies inside the CPU's clock, between the two glFinish
        // calls, so the GPU's clock never reads more than the CPU's. The
        // test holds the GPU's clock to no share of the CPU's: how much of
        // the frame llvmpipe's query covers depends on how many threads
        // llvmpipe draws with. With LP_NUM_THREADS=1 the query read 4.4 ms
        // of a 72 ms frame, with 4 threads 4.8 ms of 18 ms, and with 32
        // threads nearly all of it. CI's runners have 2 to 4 CPUs, and
        // llvmpipe starts a thread for each CPU.
        assert!(
            wall_heavy.0 >= heavy * 0.95,
            "the CPU's clock reads {} ms and the GPU's {heavy} ms",
            wall_heavy.0
        );
    }
}
