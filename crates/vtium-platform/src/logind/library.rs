//! Which library gives the `logind` feature its sd-bus, as the build
//! script chooses it. The build script includes this file and asks
//! pkg-config; the crate's tests include it and stand in for pkg-config.

use std::fmt::Display;

/// A pkg-config package that provides sd-bus, and the first version that
/// exports every call `logind.rs` declares: `sd_bus_set_method_call_timeout`
/// came with libsystemd 240 and libelogind 241.
pub struct Package {
    pub name: &'static str,
    pub min_version: &'static str,
}

/// The packages tried, in order: libsystemd, then libelogind, which exports
/// the same sd-bus ABI under its own name.
pub const PACKAGES: [Package; 2] = [
    Package {
        name: "libsystemd",
        min_version: "240",
    },
    Package {
        name: "libelogind",
        min_version: "241",
    },
];

/// The first package `find` finds, or the message that stops the build:
/// what to install, then why each package was not found.
pub fn choose<L, E: Display>(mut find: impl FnMut(&Package) -> Result<L, E>) -> Result<L, String> {
    let mut message = String::from(
        "the `logind` feature needs sd-bus from libsystemd 240 or later, \
         or libelogind 241 or later, and pkg-config found neither: install \
         libsystemd-dev or libelogind-dev, or build without `logind`",
    );
    for package in &PACKAGES {
        match find(package) {
            Ok(library) => return Ok(library),
            Err(e) => message.push_str(&format!("\n\n{}: {}", package.name, e.to_string().trim())),
        }
    }
    Err(message)
}

#[cfg(test)]
mod tests {
    #![expect(clippy::panic, reason = "a test fails by panicking")]

    use super::*;

    #[test]
    fn libsystemd_is_taken_first() {
        let mut asked = Vec::new();
        let found = choose(|p| {
            asked.push(p.name);
            Ok::<_, String>(p.name)
        });
        assert_eq!(found, Ok("libsystemd"));
        assert_eq!(asked, ["libsystemd"]);
    }

    #[test]
    fn libelogind_is_taken_without_libsystemd() {
        let mut asked = Vec::new();
        let found = choose(|p| {
            asked.push((p.name, p.min_version));
            if p.name == "libsystemd" {
                Err("not found")
            } else {
                Ok(p.name)
            }
        });
        assert_eq!(found, Ok("libelogind"));
        assert_eq!(asked, [("libsystemd", "240"), ("libelogind", "241")]);
    }

    #[test]
    fn neither_stops_the_build_with_one_message() {
        let Err(message) = choose(|p| Err::<(), _>(format!("no {}.pc", p.name))) else {
            panic!("found a library where there is none");
        };
        assert!(
            message.starts_with(
                "the `logind` feature needs sd-bus from libsystemd 240 or later, \
                 or libelogind 241 or later, and pkg-config found neither: install \
                 libsystemd-dev or libelogind-dev, or build without `logind`"
            ),
            "{message}"
        );
        assert!(
            message.contains("libsystemd: no libsystemd.pc"),
            "{message}"
        );
        assert!(
            message.contains("libelogind: no libelogind.pc"),
            "{message}"
        );
    }
}
