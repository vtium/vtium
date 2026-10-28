#!/usr/bin/env python3
"""Self-test for t3img: image I/O and masked exact comparison.

    python3 -I -B tools/tier3/test_t3img.py
"""

import os
import struct
import sys
import tempfile
import unittest
import zlib

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import t3img  # noqa: E402


def solid(w, h, rgb=(0, 0, 0)):
    return t3img.Image(w, h, bytes(rgb) * (w * h))


def with_pixel(img, x, y, rgb):
    data = bytearray(img.data)
    i = (y * img.width + x) * 3
    data[i:i + 3] = bytes(rgb)
    return t3img.Image(img.width, img.height, bytes(data))


def png_with_filter(img, ftype):
    """Encode `img` as an RGB PNG with every row using filter `ftype`."""
    stride, bpp = img.width * 3, 3
    prev = bytes(stride)
    raw = bytearray()
    for y in range(img.height):
        row = img.row(y)
        out = bytearray(stride)
        for i in range(stride):
            left = row[i - bpp] if i >= bpp else 0
            up = prev[i]
            upleft = prev[i - bpp] if i >= bpp else 0
            pred = {0: 0, 1: left, 2: up, 3: (left + up) >> 1,
                    4: t3img._paeth(left, up, upleft)}[ftype]
            out[i] = (row[i] - pred) & 0xFF
        raw += bytes([ftype]) + out
        prev = row

    def chunk(kind, payload):
        return (struct.pack(">I", len(payload)) + kind + payload +
                struct.pack(">I", zlib.crc32(kind + payload) & 0xFFFFFFFF))

    return (b"\x89PNG\r\n\x1a\n" +
            chunk(b"IHDR", struct.pack(">IIBBBBB", img.width, img.height, 8, 2, 0, 0, 0)) +
            chunk(b"IDAT", zlib.compress(bytes(raw))) + chunk(b"IEND", b""))


def gradient(w, h):
    return t3img.Image(w, h, bytes((x * 7 + y * 13 + c * 50) & 0xFF
                                   for y in range(h) for x in range(w) for c in range(3)))


class Io(unittest.TestCase):
    def test_png_round_trip(self):
        img = gradient(37, 11)
        with tempfile.TemporaryDirectory() as d:
            p = os.path.join(d, "a.png")
            t3img.write_png(p, img)
            self.assertEqual(t3img.read(p).data, img.data)

    def test_png_filters_decode(self):
        img = gradient(19, 7)
        for ftype in range(5):
            with self.subTest(filter=ftype):
                self.assertEqual(t3img.read_png(png_with_filter(img, ftype)).data, img.data)

    def test_ppm_with_comment(self):
        img = gradient(4, 3)
        buf = b"P6\n# made by a test\n4 3\n255\n" + img.data
        got = t3img.read_ppm(buf)
        self.assertEqual((got.width, got.height, got.data), (4, 3, img.data))

    def test_ppm_padded_rows(self):
        # QEMU's screendump pads each row to 4 bytes: a 5-pixel row of 15
        # bytes arrives as 16, and the extra byte is not a pixel.
        img = gradient(5, 3)
        rows = b"".join(img.row(y) + b"\xee" for y in range(3))
        got = t3img.read_ppm(b"P6\n5 3\n255\n" + rows)
        self.assertEqual(got.data, img.data)

    def test_ppm_padding_of_each_width(self):
        # Rows of 3w bytes padded to 4: widths 4, 5, 6 and 7 carry 0, 1, 2
        # and 3 bytes more per row; 1366 (4098 bytes) carries 2, as QEMU's
        # 1366x768 screendump does.
        for w in (4, 5, 6, 7, 1366):
            with self.subTest(width=w):
                img = gradient(w, 3)
                pad = (-3 * w) % 4
                rows = b"".join(img.row(y) + b"\xee" * pad for y in range(3))
                got = t3img.read_ppm(b"P6\n%d 3\n255\n" % w + rows)
                self.assertEqual(got.data, img.data)

    def test_ppm_wrong_size_refused(self):
        # A capture still being written, or one with bytes to spare, is not
        # read as a whole one.
        img = gradient(5, 3)
        for body in (img.data[:-1], img.data + b"\0", img.data + b"\0\0"):
            with self.subTest(len(body)), self.assertRaises(ValueError):
                t3img.read_ppm(b"P6\n5 3\n255\n" + body)


class Compare(unittest.TestCase):
    def test_identical_images_pass(self):
        stats, diff, _ = t3img.compare(gradient(20, 10), gradient(20, 10))
        self.assertEqual(stats, {"pixels": 0, "bbox": [], "max_delta": 0})
        self.assertIsNone(diff)

    def test_one_pixel_difference_is_found(self):
        a = solid(64, 32)
        b = with_pixel(a, 40, 20, (0, 3, 0))
        stats, diff, _ = t3img.compare(b, a)
        self.assertEqual(stats["pixels"], 1)
        self.assertEqual(stats["bbox"], [[40, 20, 41, 21]])
        self.assertEqual(stats["max_delta"], 3)
        self.assertEqual(diff.data[(20 * 64 + 40) * 3:(20 * 64 + 40) * 3 + 3], b"\xff\x00\x00")

    def test_separate_regions_get_separate_boxes(self):
        a = solid(128, 64)
        b = with_pixel(with_pixel(a, 2, 2, (9, 9, 9)), 120, 60, (9, 9, 9))
        stats, _, _ = t3img.compare(b, a)
        self.assertEqual(stats["bbox"], [[2, 2, 3, 3], [120, 60, 121, 61]])

    def test_mask_hides_difference_inside_only(self):
        a = solid(64, 32)
        b = with_pixel(with_pixel(a, 10, 10, (255, 255, 255)), 50, 10, (255, 255, 255))
        stats, _, _ = t3img.compare(b, a, [(8, 8, 16, 16)])
        self.assertEqual(stats["bbox"], [[50, 10, 51, 11]])

    def test_size_mismatch_fails(self):
        stats, diff, _ = t3img.compare(solid(10, 10), solid(10, 12))
        self.assertEqual(stats["pixels"], 120)
        self.assertIn("size_mismatch", stats)
        self.assertIsNone(diff)

    def test_nearby_pixels_share_a_region(self):
        a = solid(64, 32)
        b = a
        for x in (3, 5, 9, 14):
            b = with_pixel(b, x, 4, (1, 1, 1))
        stats, _, _ = t3img.compare(b, a)
        self.assertEqual(stats["bbox"], [[3, 4, 15, 5]])

    def test_regions_are_capped(self):
        # 100 isolated pixels far apart: clustering alone gives 100 boxes.
        points = [(x * 40, y * 40) for x in range(10) for y in range(10)]
        boxes = t3img.regions(points)
        self.assertLessEqual(len(boxes), t3img.MAX_REGIONS)
        for x, y in points:  # every pixel is still inside some region
            self.assertTrue(any(b[0] <= x < b[2] and b[1] <= y < b[3] for b in boxes))

    def test_cli_exit_status(self):
        with tempfile.TemporaryDirectory() as d:
            a, b = os.path.join(d, "a.png"), os.path.join(d, "b.png")
            t3img.write_png(a, solid(8, 8))
            t3img.write_png(b, with_pixel(solid(8, 8), 1, 1, (1, 0, 0)))
            self.assertEqual(t3img.main(["compare", a, a]), 0)
            self.assertEqual(t3img.main(["compare", b, a]), 1)
            tol = os.path.join(d, "b.tol")
            with open(tol, "w") as f:
                f.write("max_delta=1\nmax_pixels=1\n")
            self.assertEqual(t3img.main(["compare", b, a, "--tol", tol]), 0)


class Tolerance(unittest.TestCase):
    def write(self, text):
        fd, path = tempfile.mkstemp(suffix=".tol")
        with os.fdopen(fd, "w") as f:
            f.write(text)
        self.addCleanup(os.unlink, path)
        return path

    def test_no_sidecar_is_exact(self):
        self.assertIsNone(t3img.load_tol(None))
        self.assertIsNone(t3img.load_tol("/nonexistent/x.tol"))
        self.assertEqual(t3img.verdict({"pixels": 1, "max_delta": 1}, None), "fail")

    def test_parse(self):
        tol = t3img.load_tol(self.write(
            "# cursor cell\nmax_delta = 2\nmask=0,780,10,800\nmask=5,5,6,6 # dot\n"))
        self.assertEqual(tol, {"max_delta": 2, "max_pixels": 0,
                               "masks": [[0, 780, 10, 800], [5, 5, 6, 6]]})

    def test_malformed_sidecar_is_an_error(self):
        for text in ("max_delta\n", "color=3\n", "mask=1,2,3\n", "mask=5,0,1,9\n"):
            with self.subTest(text=text), self.assertRaises(ValueError):
                t3img.load_tol(self.write(text))

    def test_verdicts(self):
        tol = {"max_delta": 2, "max_pixels": 10, "masks": []}
        self.assertEqual(t3img.verdict({"pixels": 0, "max_delta": 0}, tol), "pass")
        self.assertEqual(t3img.verdict({"pixels": 10, "max_delta": 2}, tol), "near")
        self.assertEqual(t3img.verdict({"pixels": 11, "max_delta": 2}, tol), "fail")
        self.assertEqual(t3img.verdict({"pixels": 1, "max_delta": 3}, tol), "fail")
        self.assertEqual(t3img.verdict({"pixels": 1, "max_delta": 1,
                                        "size_mismatch": []}, tol), "fail")

    def test_mask_only_sidecar_still_exact_outside(self):
        tol = t3img.load_tol(self.write("mask=0,0,4,4\n"))
        a = solid(8, 8)
        stats, _, _ = t3img.compare(with_pixel(a, 1, 1, (9, 9, 9)), a, tol["masks"])
        self.assertEqual(t3img.verdict(stats, tol), "pass")
        stats, _, _ = t3img.compare(with_pixel(a, 6, 6, (9, 9, 9)), a, tol["masks"])
        self.assertEqual(t3img.verdict(stats, tol), "fail")


class Thumbnails(unittest.TestCase):
    def test_large_frame_is_downscaled(self):
        t = t3img.thumbnail(solid(7680, 40))
        self.assertLessEqual(t.width, t3img.THUMB_WIDTH)
        self.assertEqual((t.width, t.height), (480, 3))

    def test_one_pixel_difference_survives_in_diff_thumbnail(self):
        img = solid(7680, 32)
        t = t3img.diff_thumbnail(img, [(7679, 31)])
        self.assertIn(b"\xff\x00\x00", t.data)


if __name__ == "__main__":
    unittest.main()
