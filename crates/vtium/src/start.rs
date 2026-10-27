//! What start-up takes from the configuration before the seat runs: the
//! VT, whether to switch to it, the keymap, and the key repeat and
//! pointers (`src/config.c:329-361, 1352-1353, 1377-1385` at kmscon
//! b1f7a7c5).

use std::path::{Path, PathBuf};

use vtium_platform::input::{InputConfig, KeymapConfig};

use crate::config::Config;
use crate::config::keys::{
    MOUSE, SWITCHVT, VT, XKB_COMPOSE_FILE, XKB_KEYMAP, XKB_LAYOUT, XKB_MODEL, XKB_OPTIONS,
    XKB_REPEAT_DELAY, XKB_REPEAT_RATE, XKB_VARIANT,
};

/// The start's settings.
#[derive(Clone, Debug)]
pub struct Start {
    /// The VT's device the `vt` key names; `None` to find one.
    pub vt: Option<PathBuf>,
    /// Switch to the VT at start.
    pub switch: bool,
    pub keymap: KeymapConfig,
    pub input: InputConfig,
}

impl Start {
    /// The settings `config` gives; `locale` picks the compose table.
    pub fn new(config: &Config, locale: String) -> Start {
        let text = |key| config.text(key).unwrap_or_default().to_owned();
        let path = |key| config.text(key).map(PathBuf::from);
        let ms = |key| {
            config
                .number(key)
                .map_or(0, |n| u32::try_from(n).unwrap_or(u32::MAX))
        };
        Start {
            vt: config.text(&VT).map(vt_path),
            switch: config.switch(&SWITCHVT),
            keymap: KeymapConfig {
                model: text(&XKB_MODEL),
                layout: text(&XKB_LAYOUT),
                variant: text(&XKB_VARIANT),
                options: text(&XKB_OPTIONS),
                keymap_file: path(&XKB_KEYMAP),
                compose_file: path(&XKB_COMPOSE_FILE),
                locale,
            },
            input: InputConfig::new(
                ms(&XKB_REPEAT_DELAY),
                ms(&XKB_REPEAT_RATE),
                config.switch(&MOUSE),
            ),
        }
    }
}

/// The VT's device for the `vt` key: a number names `/dev/ttyN`, a name
/// is under `/dev`, a path is itself (`src/config.c:329-361`).
fn vt_path(vt: &str) -> PathBuf {
    if !vt.is_empty() && vt.bytes().all(|b| b.is_ascii_digit()) {
        PathBuf::from(format!("/dev/tty{vt}"))
    } else if vt.starts_with('.') || vt.starts_with('/') {
        PathBuf::from(vt)
    } else {
        Path::new("/dev").join(vt)
    }
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;
    use std::time::Duration;

    use super::*;
    use crate::config::load;

    fn start(args: &[&str]) -> Start {
        let args: Vec<OsString> = args.iter().map(OsString::from).collect();
        let config = load(&args, None, &mut |_: &Path| Ok(None)).config;
        Start::new(&config, "de_DE.UTF-8".to_owned())
    }

    #[test]
    fn the_defaults_take_the_rules_defaults_and_pointers() {
        let s = start(&[]);
        assert_eq!(s.vt, None);
        assert!(s.switch);
        assert_eq!(s.keymap.layout, "");
        assert_eq!(s.keymap.keymap_file, None);
        assert_eq!(s.keymap.locale, "de_DE.UTF-8");
        assert_eq!(s.input, InputConfig::new(250, 50, true));
    }

    #[test]
    fn each_key_reaches_its_setting() {
        let s = start(&[
            "--vt=3",
            "--no-switchvt",
            "--xkb-model=pc105",
            "--xkb-layout=de",
            "--xkb-variant=nodeadkeys",
            "--xkb-options=ctrl:nocaps",
            "--xkb-keymap=/etc/keymap.xkb",
            "--xkb-compose-file=/etc/Compose",
            "--xkb-repeat-delay=300",
            "--xkb-repeat-rate=40",
            "--no-mouse",
        ]);
        assert_eq!(s.vt, Some(PathBuf::from("/dev/tty3")));
        assert!(!s.switch);
        let k = &s.keymap;
        assert_eq!(
            [&k.model, &k.layout, &k.variant, &k.options],
            ["pc105", "de", "nodeadkeys", "ctrl:nocaps"]
        );
        assert_eq!(k.keymap_file, Some(PathBuf::from("/etc/keymap.xkb")));
        assert_eq!(k.compose_file, Some(PathBuf::from("/etc/Compose")));
        assert_eq!(s.input.repeat_delay, Duration::from_millis(300));
        assert_eq!(s.input.repeat_rate, Duration::from_millis(40));
        assert!(!s.input.mouse);
    }

    #[test]
    fn a_vt_is_a_number_a_name_or_a_path() {
        assert_eq!(vt_path("2"), PathBuf::from("/dev/tty2"));
        assert_eq!(vt_path("tty5"), PathBuf::from("/dev/tty5"));
        assert_eq!(vt_path("/dev/tty9"), PathBuf::from("/dev/tty9"));
        assert_eq!(vt_path("./tty"), PathBuf::from("./tty"));
    }
}
