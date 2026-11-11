//! What start-up takes from the configuration before the seat runs: the
//! log's level, the VT, whether to switch to it, the keymap, and the key
//! repeat and pointers, with the pointer's hide timeout.

use std::path::{Path, PathBuf};

use std::time::Duration;

use vtium_platform::input::{Direct, Input, InputConfig, Keymap, KeymapConfig, RuleNames};

use crate::config::Config;
use crate::config::keys::{
    DEBUG, MOUSE, POINTER_HIDE_TIMEOUT, SWITCHVT, VT, XKB_COMPOSE_FILE, XKB_KEYMAP, XKB_LAYOUT,
    XKB_MODEL, XKB_OPTIONS, XKB_REPEAT_DELAY, XKB_REPEAT_RATE, XKB_VARIANT,
};

/// The start's settings.
#[derive(Clone, Debug)]
pub struct Start {
    /// The log's level: debug messages too under `debug`, else information
    /// messages and above.
    pub log: log::Level,
    /// The VT's device the `vt` key names; `None` to find one.
    pub vt: Option<PathBuf>,
    /// Switch to the VT at start.
    pub switch: bool,
    /// The `xkb-*` keys, which [`Start::input`] alone reads.
    keymap: KeymapConfig,
    /// The repeat, `mouse` and the pointer's hide, which [`Start::input`]
    /// alone reads.
    input: InputConfig,
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
            log: if config.switch(&DEBUG) {
                log::Level::Debug
            } else {
                log::Level::Info
            },
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
            )
            .with_pointer_hide(
                config
                    .number(&POINTER_HIDE_TIMEOUT)
                    .filter(|&s| s > 0)
                    .map(Duration::from_secs),
            ),
        }
    }
}

impl Start {
    /// The seat's input: the keymap the `xkb-*` keys name, or the
    /// built-in one when it does not compile, with the key repeat and
    /// pointers the keys set. `None` when no keymap compiles.
    ///
    /// The `xkb-*` keys that vtium.conf leaves unset take `system`, the
    /// system's keyboard layout, which vtium-platform's `system_layout`
    /// reads from the console keymap and, in a build that links logind,
    /// from localed (ADR-1400's L2 layer).
    pub fn input(&self, system: &RuleNames) -> Option<Input> {
        self.build(system, |keymap, input| {
            Keymap::new(keymap)
                .or_else(Keymap::fallback)
                .map(|k| Input::new(Direct, k, input))
        })
    }

    /// vtium.conf's own keymap settings, before the system's layout fills
    /// the `xkb-*` keys it leaves unset. A build with `logind` follows a
    /// change to the system's layout with them.
    pub fn keymap_settings(&self) -> KeymapConfig {
        self.keymap.clone()
    }

    /// The keymap's settings: the `xkb-*` keys, with `system`'s names in
    /// place of those vtium.conf leaves unset.
    fn keymap(&self, system: &RuleNames) -> KeymapConfig {
        self.keymap.clone().follow(system)
    }

    /// Hands `make` the keymap's and the input's settings: the one place
    /// they are read for the seat.
    fn build<T>(
        &self,
        system: &RuleNames,
        make: impl FnOnce(&KeymapConfig, InputConfig) -> T,
    ) -> T {
        make(&self.keymap(system), self.input)
    }
}

/// The VT's device for the `vt` key: a number names `/dev/ttyN`, a name
/// is under `/dev`, a path is itself.
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
        let config = load(&args, &mut |_: &Path| Ok(None)).config;
        Start::new(&config, "de_DE.UTF-8".to_owned())
    }

    // ADR-1400's L2 layer: the system's layout, as localed names it, fills
    // the `xkb-*` keys vtium.conf leaves unset; a key vtium.conf sets
    // wins, and without the system's names the keys stay as they are.
    #[test]
    fn the_systems_layout_fills_the_xkb_keys_vtium_conf_leaves_unset() {
        let system = RuleNames {
            model: "pc105".to_owned(),
            layout: "de".to_owned(),
            variant: "nodeadkeys".to_owned(),
            options: "caps:escape".to_owned(),
        };
        let unset = start(&[]).keymap(&system);
        assert_eq!(
            (
                unset.model.as_str(),
                unset.layout.as_str(),
                unset.variant.as_str(),
                unset.options.as_str()
            ),
            ("pc105", "de", "nodeadkeys", "caps:escape")
        );
        let set = start(&["--xkb-layout=fr"]).keymap(&system);
        assert_eq!(
            (
                set.layout.as_str(),
                set.variant.as_str(),
                set.model.as_str()
            ),
            ("fr", "", "pc105"),
            "vtium.conf's layout wins, with no variant of the system's"
        );
        assert_eq!(
            start(&[]).keymap(&RuleNames::default()).layout,
            "",
            "no names, no layout"
        );
    }

    #[test]
    fn the_defaults_take_the_rules_defaults_and_pointers() {
        let s = start(&[]);
        assert_eq!(s.log, log::Level::Info);
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
        assert_eq!(start(&[]).input.pointer_hide, Some(Duration::from_secs(20)));
        let s = start(&["--pointer-hide-timeout=3"]);
        assert_eq!(s.input.pointer_hide, Some(Duration::from_secs(3)));
        assert_eq!(
            start(&["--pointer-hide-timeout=0"]).input.pointer_hide,
            None
        );
    }

    /// The seat's input is made from the keys, through the one function
    /// `main` calls.
    #[test]
    fn the_input_is_made_from_the_keys() {
        let s = start(&["--xkb-layout=de", "--xkb-repeat-delay=300", "--no-mouse"]);
        let (layout, input) = s.build(&RuleNames::default(), |keymap, input| {
            (keymap.layout.clone(), input)
        });
        assert_eq!(layout, "de");
        assert_eq!(input, InputConfig::new(300, 50, false));
        assert!(
            s.input(&RuleNames::default()).is_some(),
            "a keymap, the built-in one at worst"
        );
    }

    #[test]
    fn debug_logs_debug_messages_set_in_a_file_or_on_the_command_line() {
        assert_eq!(start(&["--debug"]).log, log::Level::Debug);
        assert_eq!(start(&["--no-debug"]).log, log::Level::Info);
        // `debug` in the file at `file`: vtium.conf sets it, and the seat's
        // file beside it is never read (ADR-1403 D2)
        let from = |file: &str, args: &[&str]| {
            let args: Vec<OsString> = args.iter().map(OsString::from).collect();
            let loaded = load(&args, &mut |p: &Path| {
                Ok((p == Path::new(file)).then(|| b"debug\n".to_vec()))
            });
            assert_eq!(loaded.reports, []);
            Start::new(&loaded.config, "C".to_owned()).log
        };
        let conf = "/etc/vtium/vtium.conf";
        assert_eq!(from(conf, &[]), log::Level::Debug);
        assert_eq!(from(conf, &["--no-debug"]), log::Level::Info);
        assert_eq!(from("/etc/vtium/seat0.seat.conf", &[]), log::Level::Info);
    }

    #[test]
    fn a_vt_is_a_number_a_name_or_a_path() {
        assert_eq!(vt_path("2"), PathBuf::from("/dev/tty2"));
        assert_eq!(vt_path("tty5"), PathBuf::from("/dev/tty5"));
        assert_eq!(vt_path("/dev/tty9"), PathBuf::from("/dev/tty9"));
        assert_eq!(vt_path("./tty"), PathBuf::from("./tty"));
    }
}
