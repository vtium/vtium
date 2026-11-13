//! Feature queries, each test written from ADR-1100 D14.

use super::*;

/// The answer to feature query `keys`, sent alone.
fn ask(keys: &str) -> String {
    let (mut k, mut h) = (Kitty::default(), Host::new());
    send(&mut k, &mut h, keys);
    h.take()
}

// covers: image.kitty-feature-query
#[test]
fn the_specifications_two_examples_get_its_own_answers() {
    assert_eq!(
        ask("a=q,i=31,e=T,U=1,C=1"),
        "\x1b_Gi=31,e=T,C=1,U=1;OK\x1b\\"
    );
    // vtium takes X=1 on a frame, so it reports it too
    assert_eq!(
        ask("a=q,i=31,e=f,r=1,X=1"),
        "\x1b_Gi=31,e=f,X=1,r=1;OK\x1b\\"
    );
}

// covers: image.kitty-feature-query
#[test]
fn each_action_reports_the_keys_and_values_a_command_of_it_takes() {
    let cases = [
        (
            "a=q,i=32,e=a,s=3,v=1,c=2,r=1,z=40",
            "\x1b_Gi=32,e=a,c=2,r=1,s=3,v=1,z=40;OK\x1b\\",
        ),
        (
            "a=q,i=33,e=c,r=1,c=2,w=1,h=1,C=1",
            "\x1b_Gi=33,e=c,C=1,c=2,h=1,r=1,w=1;OK\x1b\\",
        ),
        (
            "a=q,i=34,e=t,f=100,t=s,o=z,S=10,O=2,N=1",
            "\x1b_Gi=34,e=t,N=1,O=2,S=10,f=100,o=z,t=s;OK\x1b\\",
        ),
        (
            "a=q,i=39,e=p,P=1,Q=2,H=1,V=-1,p=7,z=-5",
            "\x1b_Gi=39,e=p,H=1,P=1,Q=2,V=-1,p=7,z=-5;OK\x1b\\",
        ),
        ("a=q,i=40,e=d,d=Z,z=3", "\x1b_Gi=40,e=d,d=Z,z=3;OK\x1b\\"),
        ("a=q,i=46,e=q,f=24,s=2", "\x1b_Gi=46,e=q,f=24,s=2;OK\x1b\\"),
    ];
    for (keys, want) in cases {
        assert_eq!(ask(keys), want, "{keys}");
    }
}

// covers: image.kitty-feature-query
#[test]
fn palette_indices_are_reported_for_a_picture_and_not_for_a_frame() {
    assert_eq!(ask("a=q,i=35,e=t,f=8"), "\x1b_Gi=35,e=t,f=8;OK\x1b\\");
    assert_eq!(ask("a=q,i=35,e=T,f=16"), "\x1b_Gi=35,e=T,f=16;OK\x1b\\");
    assert_eq!(ask("a=q,i=36,e=f,f=8"), "\x1b_Gi=36,e=f;OK\x1b\\");
}

// covers: image.kitty-feature-query
#[test]
fn defaults_values_vtium_refuses_and_keys_the_action_does_not_read_are_left_out() {
    assert_eq!(ask("a=q,i=37,e=t,f=32,t=d"), "\x1b_Gi=37,e=t;OK\x1b\\");
    assert_eq!(ask("a=q,i=41,e=d,d=a"), "\x1b_Gi=41,e=d;OK\x1b\\");
    assert_eq!(ask("a=q,i=38,e=t,U=1,C=1,c=3"), "\x1b_Gi=38,e=t;OK\x1b\\");
    assert_eq!(
        ask("a=q,i=44,e=T,U=2,C=7,f=7,t=x,s=x"),
        "\x1b_Gi=44,e=T;OK\x1b\\"
    );
    // a side past max-graphic-size, a box past BOX_CELLS_MAX, an offset
    // past the 8×16 cell
    assert_eq!(
        ask("a=q,i=45,e=T,s=65536,c=10001,X=8,Y=15"),
        "\x1b_Gi=45,e=T,Y=15;OK\x1b\\"
    );
    // a, i, I, m and q are never reported
    assert_eq!(ask("a=q,i=47,e=t,m=1,q=0,I=3"), "\x1b_Gi=47,e=t;OK\x1b\\");
}

// covers: image.kitty-feature-query
#[test]
fn an_action_vtium_does_not_know_is_answered_without_e() {
    assert_eq!(ask("a=q,i=43,e=Z,U=1"), "\x1b_Gi=43;OK\x1b\\");
    assert_eq!(ask("a=q,i=43,e=TT,U=1"), "\x1b_Gi=43;OK\x1b\\");
}

// covers: image.kitty-feature-query
#[test]
fn a_feature_query_without_i_or_under_q_gets_no_answer() {
    assert_eq!(ask("a=q,e=T,U=1"), "");
    assert_eq!(ask("a=q,I=5,e=T,U=1"), "");
    assert_eq!(ask("a=q,i=45,e=T,U=1,q=1"), "");
}

// covers: image.kitty-feature-query
#[test]
fn a_feature_query_ignores_its_payload_and_m_and_stores_nothing() {
    let (mut k, mut h) = (Kitty::default(), Host::new());
    send(&mut k, &mut h, "a=q,i=47,e=t,f=24,s=1,v=1;AAAA");
    send(&mut k, &mut h, "a=p,i=47,C=1");
    assert_eq!(
        h.take(),
        "\x1b_Gi=47,e=t,f=24,s=1,v=1;OK\x1b\\\x1b_Gi=47;ENOENT:Put command refers to non-existent image with id: 47 and number: 0\x1b\\"
    );
    // m=1 opens no upload: the next command is its own, with no id
    send(&mut k, &mut h, "a=q,i=52,e=T,U=1,m=1");
    send(&mut k, &mut h, "m=0");
    assert_eq!(h.take(), "\x1b_Gi=52,e=T,U=1;OK\x1b\\");
    assert_eq!(h.store.bytes(), 0);
}

// covers: image.kitty-feature-query
#[test]
fn a_feature_query_leaves_an_open_upload_as_it_was() {
    let (mut k, mut h) = (Kitty::default(), Host::new());
    send(&mut k, &mut h, "a=t,f=24,s=2,v=1,i=1,m=1;/wAA");
    send(&mut k, &mut h, "a=q,i=51,e=T,U=1");
    send(&mut k, &mut h, "m=0;AP8A");
    assert_eq!(h.take(), "\x1b_Gi=51,e=T,U=1;OK\x1b\\\x1b_Gi=1;OK\x1b\\");
}

// covers: image.kitty-feature-query
#[test]
fn e_on_an_action_other_than_a_query_is_refused_and_does_nothing() {
    let (mut k, mut h) = (Kitty::default(), Host::new());
    send(&mut k, &mut h, "a=t,i=48,e=T,f=24,s=1,v=1;AAAA");
    send(&mut k, &mut h, "a=p,i=48,C=1");
    assert_eq!(
        h.take(),
        "\x1b_Gi=48;EINVAL:invalid argument\x1b\\\x1b_Gi=48;ENOENT:Put command refers to non-existent image with id: 48 and number: 0\x1b\\"
    );
    assert_eq!(h.store.bytes(), 0);
    // a refused delete does not end an open upload; it names no image,
    // so its refusal goes unanswered
    send(&mut k, &mut h, "a=t,f=24,s=2,v=1,i=1,m=1;/wAA");
    send(&mut k, &mut h, "a=d,e=d");
    send(&mut k, &mut h, "m=0;AP8A");
    assert_eq!(h.take(), "\x1b_Gi=1;OK\x1b\\");
}

// The widest answer a feature query can get fits KITTY_REPLY_MAX: e=T
// reports the most keys, each at its widest value vtium takes, with the
// largest cell and max-graphic-size vtium allows.
// covers: image.kitty-feature-query
#[test]
fn the_widest_feature_query_answer_fits_the_answer_bound() {
    let (mut k, mut h) = (Kitty::default(), Host::new());
    h.store.set_cell(1024, 1024).unwrap();
    h.limits.graphic = (65_535, 65_535);
    let big = u32::MAX;
    let low = i32::MIN;
    send(
        &mut k,
        &mut h,
        &format!(
            "a=q,i={big},e=T,f=100,t=f,o=z,s=65535,v=65535,S={big},O={big},N=1,\
             x={big},y={big},w={big},h={big},X=1023,Y=1023,c=10000,r=10000,C=1,U=1,\
             z={low},P={big},Q={big},H={low},V={low},p={big}"
        ),
    );
    let answer = h.take();
    // every key is reported, so no answer of e=T is longer
    for key in [
        "N", "O", "S", "X", "Y", "f", "o", "s", "t", "v", "x", "y", "w", "h", "c", "r", "C", "U",
        "z", "P", "Q", "H", "V", "p",
    ] {
        assert!(answer.contains(&format!(",{key}=")), "{key} in {answer:?}");
    }
    assert!(answer.ends_with(";OK\x1b\\"), "{answer:?}");
    assert!(
        u64::try_from(answer.len()).unwrap() <= crate::KITTY_REPLY_MAX,
        "{} bytes",
        answer.len()
    );
}
