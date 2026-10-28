//! How many levels a DRM card's panel shows, 2 for a 1-bit panel and 4 or
//! 16 for a gray one, from the driver's name, which `DRM_IOCTL_VERSION`
//! gives, and the panel's device-tree node (vtium/vtium#135).
//!
//! No connector or plane property says a panel is 1-bit or gray: each
//! driver below offers XRGB8888 and turns every frame into its panel's
//! levels itself, from the frame's BT.601 gray. The sources are Linux
//! v7.3's `drivers/gpu/drm/`. `gud` is not listed: it drives color devices
//! as well as 1-bit ones, whose format it keeps from userspace
//! (`gud/gud_drv.c:569-570`).

/// A driver's panel's levels.
#[derive(Clone, Copy, Debug)]
enum Levels {
    Always(u8),
    /// By the node's `compatible`: the levels of the first panel listed
    /// that it names, and else the default; 0 for a color panel.
    Compatible(&'static [(&'static str, u8)], u8),
    /// The first with the node's property, the second without.
    Property(&'static str, u8, u8),
}

/// The DRM drivers of 1-bit and gray panels: the name in
/// `struct drm_driver`, and its panel's levels.
const DRIVERS: [(&str, Levels); 6] = [
    // solomon/ssd130x.c:42 and :1687; the SSD130x family converts to 1 bit
    // (:943), the SSD132x family to gray8 whose high nibble it sends
    // (:968, :807) and the SSD1331 to RGB332 (:196-216), told apart by
    // compatible (ssd130x-i2c.c, ssd130x-spi.c)
    (
        "ssd130x",
        Levels::Compatible(
            &[
                ("solomon,ssd1322", 16),
                ("solomon,ssd1325", 16),
                ("solomon,ssd1327", 16),
                ("solomon,ssd1331", 0),
            ],
            2,
        ),
    ),
    // tiny/repaper.c:917, converted at :549
    ("repaper", Levels::Always(2)),
    // tiny/sharp-memory.c:107, converted at :142
    ("sharp_memory_display", Levels::Always(2)),
    // sitronix/st7920.c:31 and :678, R1 at :366
    ("sitronix_st7920", Levels::Always(2)),
    // sitronix/st7571.c:78 and :601: black and white (:298-307) unless the
    // node asks for its 2-bit gray mode (:693), converted by
    // drm_format_helper.c:1168-1185
    ("st7571", Levels::Property("sitronix,grayscale", 4, 2)),
    // sitronix/st7586.c:342: gray8's top two bits (:65-77, :103-105)
    ("st7586", Levels::Always(4)),
];

/// The levels of the panel the card whose driver is `driver` drives, or
/// `None` for a color display. `property` reads a property of the
/// device's device-tree node, `None` where it or the node is missing;
/// `compatible` is NUL-separated. A node that cannot be read leaves the
/// driver's own default: the SSD130x family for `ssd130x`, black and white
/// for `st7571`.
pub(crate) fn levels(driver: &str, property: impl Fn(&str) -> Option<Vec<u8>>) -> Option<u8> {
    let &(_, levels) = DRIVERS.iter().find(|(name, _)| *name == driver)?;
    let n = match levels {
        Levels::Always(n) => n,
        Levels::Compatible(panels, default) => {
            let compatible = property("compatible").unwrap_or_default();
            compatible
                .split(|&b| b == 0)
                .find_map(|c| panels.iter().find(|(p, _)| p.as_bytes() == c))
                .map_or(default, |&(_, n)| n)
        }
        Levels::Property(p, with, without) => {
            if property(p).is_some() {
                with
            } else {
                without
            }
        }
    };
    (n >= 2).then_some(n)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node<'a>(props: &'a [(&'a str, &'a [u8])]) -> impl Fn(&str) -> Option<Vec<u8>> + 'a {
        move |name| {
            props
                .iter()
                .find(|(n, _)| *n == name)
                .map(|(_, v)| v.to_vec())
        }
    }

    #[test]
    fn the_listed_drivers_give_their_levels() {
        for d in ["repaper", "sharp_memory_display", "sitronix_st7920"] {
            assert_eq!(levels(d, node(&[])), Some(2), "{d}");
        }
        assert_eq!(levels("st7586", node(&[])), Some(4));
        for d in ["i915", "amdgpu", "simpledrm", "gud", "", "ssd130"] {
            assert_eq!(levels(d, node(&[])), None, "{d}");
        }
    }

    #[test]
    fn ssd130x_is_1_bit_but_for_its_gray_and_color_panels() {
        assert_eq!(levels("ssd130x", node(&[])), Some(2));
        let ssd1306 = b"solomon,ssd1306fb-i2c\0".as_slice();
        assert_eq!(levels("ssd130x", node(&[("compatible", ssd1306)])), Some(2));
        let ssd1327 = b"vendor,board\0solomon,ssd1327\0".as_slice();
        assert_eq!(
            levels("ssd130x", node(&[("compatible", ssd1327)])),
            Some(16)
        );
        let ssd1331 = b"solomon,ssd1331\0".as_slice();
        assert_eq!(levels("ssd130x", node(&[("compatible", ssd1331)])), None);
    }

    #[test]
    fn st7571_is_1_bit_unless_its_node_asks_for_gray() {
        assert_eq!(levels("st7571", node(&[])), Some(2));
        let gray = [("sitronix,grayscale", b"".as_slice())];
        assert_eq!(levels("st7571", node(&gray)), Some(4));
    }
}
