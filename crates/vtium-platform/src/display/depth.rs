//! Whether a DRM card drives a 1-bit panel, from the driver's name, which
//! `DRM_IOCTL_VERSION` gives, and the panel's device-tree node (vtium/vtium#135).
//!
//! No connector or plane property says a panel is 1-bit: each driver below
//! offers XRGB8888 and turns every frame into one bit a pixel itself. The
//! sources are Linux v7.3's `drivers/gpu/drm/`. Two drivers that turn
//! XRGB8888 into fewer bits are not listed, since their panels show shades:
//! `st7586` packs 2-bit gray (`sitronix/st7586.c:65-77`), and `gud` drives
//! color devices as well as 1-bit ones, whose format it keeps from userspace
//! (`gud/gud_drv.c:569-570`).

/// When a driver's panel is 1-bit.
#[derive(Clone, Copy, Debug)]
enum When {
    Always,
    /// Unless the node's `compatible` names one of these panels, which
    /// show shades or color.
    NotCompatible(&'static [&'static str]),
    /// Unless the node has this property.
    Without(&'static str),
}

/// The DRM drivers of 1-bit panels: the name in `struct drm_driver`, and
/// when its panel is 1-bit.
const DRIVERS: [(&str, When); 5] = [
    // solomon/ssd130x.c:42 and :1687; the SSD130x family converts to 1 bit
    // (:943), the SSD132x family to 4-bit gray and the SSD1331 to RGB332
    // (:196-216), told apart by compatible (ssd130x-i2c.c, ssd130x-spi.c)
    (
        "ssd130x",
        When::NotCompatible(&[
            "solomon,ssd1322",
            "solomon,ssd1325",
            "solomon,ssd1327",
            "solomon,ssd1331",
        ]),
    ),
    // tiny/repaper.c:917, converted at :549
    ("repaper", When::Always),
    // tiny/sharp-memory.c:107, converted at :142
    ("sharp_memory_display", When::Always),
    // sitronix/st7920.c:31 and :678, R1 at :366
    ("sitronix_st7920", When::Always),
    // sitronix/st7571.c:78 and :601: black and white (:298-307) unless the
    // node asks for its 2-bit gray mode (:693)
    ("st7571", When::Without("sitronix,grayscale")),
];

/// Whether the card whose driver is `driver` drives a 1-bit panel.
/// `property` reads a property of the device's device-tree node, `None`
/// where it or the node is missing; `compatible` is NUL-separated. A
/// node that cannot be read leaves the driver's own default: the SSD130x
/// family for `ssd130x`, black and white for `st7571`.
pub(crate) fn monochrome(driver: &str, property: impl Fn(&str) -> Option<Vec<u8>>) -> bool {
    let Some(&(_, when)) = DRIVERS.iter().find(|(name, _)| *name == driver) else {
        return false;
    };
    match when {
        When::Always => true,
        When::NotCompatible(shades) => {
            let compatible = property("compatible").unwrap_or_default();
            !compatible
                .split(|&b| b == 0)
                .any(|c| shades.iter().any(|s| s.as_bytes() == c))
        }
        When::Without(p) => property(p).is_none(),
    }
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
    fn the_listed_drivers_are_1_bit() {
        for d in ["repaper", "sharp_memory_display", "sitronix_st7920"] {
            assert!(monochrome(d, node(&[])), "{d}");
        }
        for d in ["i915", "amdgpu", "simpledrm", "st7586", "gud", "", "ssd130"] {
            assert!(!monochrome(d, node(&[])), "{d}");
        }
    }

    #[test]
    fn ssd130x_is_1_bit_but_for_its_gray_and_color_panels() {
        assert!(monochrome("ssd130x", node(&[])));
        let ssd1306 = b"solomon,ssd1306fb-i2c\0".as_slice();
        assert!(monochrome("ssd130x", node(&[("compatible", ssd1306)])));
        for c in [
            &b"solomon,ssd1327\0"[..],
            b"vendor,board\0solomon,ssd1331\0",
        ] {
            assert!(!monochrome("ssd130x", node(&[("compatible", c)])));
        }
    }

    #[test]
    fn st7571_is_1_bit_unless_its_node_asks_for_gray() {
        assert!(monochrome("st7571", node(&[])));
        assert!(!monochrome(
            "st7571",
            node(&[("sitronix,grayscale", b"".as_slice())])
        ));
    }
}
