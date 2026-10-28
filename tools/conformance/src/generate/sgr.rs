//! SGR: attributes, colors in each form SGR takes, underline styles and
//! colors, and the sub-parameters only SGR reads.

use super::{CSI, Case, Inputs, Made, case, cup, sgr};

pub(super) const CASES: &[Case] = &[
    case("sgr-basic", sgr_basic),
    case("sgr-extended-color", sgr_extended_color),
    case("sgr-underline-extended", sgr_underline_extended),
    case("sgr-color-forms-fg", |_| color_forms(38, "").into()),
    case("sgr-color-forms-bg", |_| color_forms(48, "").into()),
    // the underline color on a single underline
    case("sgr-color-forms-ul", |_| color_forms(58, "4;").into()),
    case("sgr-underline-styles", sgr_underline_styles),
    case("sgr-strike", sgr_strike),
    case("sgr-underline-color-reset", sgr_underline_color_reset),
    case("sgr-subparam-unknown", sgr_subparam_unknown),
    case("sgr-color-colon-out-of-range", sgr_color_colon_out_of_range),
    case(
        "sgr-color-semicolon-subparams",
        sgr_color_semicolon_subparams,
    ),
    case("sgr-58-semicolon-no-blink", sgr_58_semicolon_no_blink),
    // sub-parameters past the eighth are dropped; the parameter and the
    // rest of the sequence still apply
    case("sgr-subparam-overflow", |_| {
        [CSI, "38:2:0:10:20:30:0:0:0:99:99;1m", "bold", &sgr(&[0])]
            .concat()
            .into()
    }),
    case("csi-subparam-non-sgr", csi_subparam_non_sgr),
    case("colour-resolve", color_resolve),
];

fn sgr_basic(_: &Inputs) -> Made {
    let off = sgr(&[0]);
    let mut out = String::from("plain ");
    for (p, label) in [
        (1, "bold"),
        (2, "dim"),
        (3, "italic"),
        (4, "underline"),
        (5, "blink"),
        (7, "inverse"),
        (8, "conceal"),
        (9, "strike"),
    ] {
        out += &[&sgr(&[p]), label, &off, " "].concat();
    }
    for (range, label) in [(30..38, "fg"), (40..48, "bg"), (90..98, "hi")] {
        out += "\r\n";
        for c in range {
            out += &format!("{}{label}{c}{off} ", sgr(&[c]));
        }
    }
    out += &[
        "\r\n",
        &sgr(&[1, 4, 31, 42]),
        "combined",
        &sgr(&[22]),
        "nobold",
        &sgr(&[24]),
        "noul",
        &sgr(&[39, 49]),
        "default",
        &off,
    ]
    .concat();
    out.into()
}

fn sgr_extended_color(_: &Inputs) -> Made {
    let mut out = String::new();
    for n in [16, 88, 196, 231, 232, 255] {
        out += &format!(
            "{}f{n}{}b{n}{} ",
            sgr(&[38, 5, n]),
            sgr(&[48, 5, n]),
            sgr(&[0])
        );
    }
    out += &[
        "\r\n",
        &sgr(&[38, 2, 255, 128, 0]),
        "orange",
        &sgr(&[48, 2, 0, 0, 128]),
        "onnavy",
        &sgr(&[0]),
    ]
    .concat();
    out.into()
}

/// Colon sub-parameters: styled and colored underlines (SGR 4:x, 58, 59).
fn sgr_underline_extended(_: &Inputs) -> Made {
    let off = sgr(&[0]);
    [
        CSI,
        "4:3m",
        "curly",
        &off,
        " ",
        CSI,
        "4:2m",
        "double",
        &off,
        " ",
        CSI,
        "1;4:3m",
        "boldcurly",
        &off,
        " ",
        CSI,
        "4;58:2::255:0:0m",
        "redul",
        CSI,
        "59m",
        "nocolor",
        &off,
        " ",
        CSI,
        "4;58;5;196m",
        "ul196",
        &off,
        " ",
        CSI,
        "4:0m",
        "ulnone",
        &off,
    ]
    .concat()
    .into()
}

/// Each color form SGR 38, 48 and 58 accept, one labelled run each, with
/// a distinct color per form, so a form read wrong shows as a wrong value.
fn color_forms(selector: u32, prefix: &str) -> String {
    let forms = [
        // colon palette index, 16-255: resolved to RGB
        ("c5", ":5:196"),
        // colon palette index below 16: stays a code
        ("c5lo", ":5:9"),
        // semicolon palette index
        ("s5", ";5;33"),
        // colon RGB without a color-space slot
        ("c2", ":2:10:20:30"),
        // colon RGB, color space empty (T.416)
        ("c2e", ":2::40:50:60"),
        // colon RGB with a color space
        ("c2cs", ":2:0:70:80:90"),
        // trailing T.416 fields ignored
        ("c2t", ":2:0:100:110:120:7:8:9"),
        // semicolon RGB takes exactly 3; the 1 is bold
        ("s2", ";2;130;140;150;1"),
    ];
    forms
        .iter()
        .map(|(label, form)| format!("{CSI}{prefix}{selector}{form}m{label}{} ", sgr(&[0])))
        .collect()
}

/// 4:0-4:5, an unknown style (single), plain 4, 21 double, 24 none, and a
/// style replaced by plain 4.
fn sgr_underline_styles(_: &Inputs) -> Made {
    let off = sgr(&[0]);
    let mut out: String = [0, 1, 2, 3, 4, 5, 9]
        .iter()
        .map(|n| format!("{CSI}4:{n}ms{n}{off} "))
        .collect();
    out += &[
        &sgr(&[4]),
        "plain",
        &off,
        " ",
        &sgr(&[21]),
        "dbl21",
        &off,
        " ",
        CSI,
        "4:3m",
        "curly",
        &sgr(&[24]),
        "off24",
        " ",
        CSI,
        "4:3m",
        "curly",
        &sgr(&[4]),
        "single",
        &off,
    ]
    .concat();
    out.into()
}

/// 9 on, 29 off, strike beside other attributes, SGR 0 clearing it.
fn sgr_strike(_: &Inputs) -> Made {
    [
        &sgr(&[9]),
        "struck",
        &sgr(&[29]),
        "clear",
        " ",
        &sgr(&[1, 9, 31]),
        "boldred",
        &sgr(&[22]),
        "red",
        &sgr(&[0]),
        "reset",
    ]
    .concat()
    .into()
}

/// 59 clears the color and keeps the style; 24 clears the style and keeps
/// the color; SGR 0 clears both.
fn sgr_underline_color_reset(_: &Inputs) -> Made {
    [
        CSI,
        "4:3;58:5:196m",
        "curlyred",
        CSI,
        "59m",
        "curly",
        " ",
        CSI,
        "4:2;58:5:2m",
        "dblgreen",
        &sgr(&[24]),
        "colouronly",
        &sgr(&[4]),
        "single",
        " ",
        CSI,
        "4:3;58;2;1;2;3m",
        "set",
        &sgr(&[0]),
        "reset",
    ]
    .concat()
    .into()
}

/// A parameter with sub-parameters SGR does not know is skipped on its
/// own; the rest of the sequence still applies.
fn sgr_subparam_unknown(_: &Inputs) -> Made {
    let off = sgr(&[0]);
    [
        // italic with a sub-parameter
        CSI,
        "1;3:1;4m",
        "boldul",
        &off,
        " ",
        // underline with two
        CSI,
        "4:3:1;1m",
        "bold",
        &off,
        " ",
        // an unknown color form
        CSI,
        "38:9:1;1m",
        "bold",
        &off,
        " ",
        // the colon palette form missing its index
        CSI,
        "38:5;3m",
        "italic",
        &off,
        " ",
        CSI,
        "7:1;9m",
        "strike",
        &off,
    ]
    .concat()
    .into()
}

/// In the colon forms an index or component above 255 rejects the color
/// alone; 255 itself is accepted.
fn sgr_color_colon_out_of_range(_: &Inputs) -> Made {
    let off = sgr(&[0]);
    [
        CSI,
        "48:2:256:300:65535;1m",
        "bold",
        &off,
        " ",
        CSI,
        "38:5:256;3m",
        "italic",
        &off,
        " ",
        CSI,
        "4;58:2::0:0:256m",
        "ul",
        &off,
        " ",
        CSI,
        "38:2:255:255:255;48:5:255m",
        "max",
        &off,
    ]
    .concat()
    .into()
}

/// A semicolon color form whose arguments carry sub-parameters is not that
/// form: nothing is consumed, and the arguments are read as SGR on their
/// own.
fn sgr_color_semicolon_subparams(_: &Inputs) -> Made {
    let off = sgr(&[0]);
    [
        // 5:1 skipped, 9 strike
        CSI,
        "38;5:1;9m",
        "strike",
        &off,
        " ",
        // 2 dim, 1 bold, 3:4 skipped
        CSI,
        "48;2;1;2;3:4m",
        "dimbold",
        &off,
    ]
    .concat()
    .into()
}

/// 58 consumes its own 5 and index; a later 5 is still blink.
fn sgr_58_semicolon_no_blink(_: &Inputs) -> Made {
    [
        &sgr(&[4, 58, 5, 196]),
        "ul196",
        &sgr(&[0]),
        " ",
        &sgr(&[4, 58, 2, 1, 2, 3, 5]),
        "blink",
        &sgr(&[0]),
    ]
    .concat()
    .into()
}

/// Only SGR reads sub-parameters: any other sequence carrying them is
/// ignored whole.
fn csi_subparam_non_sgr(_: &Inputs) -> Made {
    [&cup(2, 3), CSI, "5:1H", "here", CSI, "1:2C", "next"]
        .concat()
        .into()
}

/// Colors as a renderer resolves them: bold brightens a base color, dim
/// dims a default underline but not a set one, reverse video swaps the
/// default underline with the foreground but not a set one.
fn color_resolve(_: &Inputs) -> Made {
    let off = sgr(&[0]);
    [
        &sgr(&[1, 31]),
        "BR",
        &off,
        " ",
        &sgr(&[2, 4]),
        "DU",
        &off,
        " ",
        &sgr(&[2, 4, 58, 2, 10, 20, 30]),
        "DS",
        &off,
        " ",
        &sgr(&[7, 4]),
        "RU",
        &off,
        " ",
        &sgr(&[7, 4, 58, 5, 1]),
        "RS",
        &off,
        " ",
        &sgr(&[38, 5, 196]),
        "I",
        &off,
        " ",
        &sgr(&[38, 2, 1, 2, 3]),
        "T",
        &off,
        " ",
        &sgr(&[1, 38, 5, 1]),
        "B1",
        &off,
    ]
    .concat()
    .into()
}
