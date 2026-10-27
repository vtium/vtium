/*
 * vtium tier 2 - vtium's unifont screens drawn by the C reference
 *
 * Copyright (c) 2026 Aaron Bockelie
 *
 * Permission is hereby granted, free of charge, to any person obtaining
 * a copy of this software and associated documentation files
 * (the "Software"), to deal in the Software without restriction, including
 * without limitation the rights to use, copy, modify, merge, publish,
 * distribute, sublicense, and/or sell copies of the Software, and to
 * permit persons to whom the Software is furnished to do so, subject to
 * the following conditions:
 *
 * The above copyright notice and this permission notice shall be included
 * in all copies or substantial portions of the Software.
 *
 * THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS
 * OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
 * MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT.
 * IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
 * CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT,
 * TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE
 * SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.
 */

/*
 * Draws the screens `tier2 screens` prints, each into OUT/<name>.ppm,
 * through the C reference's own pipeline: libtsm parses the bytes,
 * kmscon_text_draw() reads the screen, bbulk draws it with the unifont
 * backend, and drm2d's blend and clear write an XRGB8888 buffer. The flow
 * is tests/test_pixels.c's, with unifont in place of FreeType:
 *
 *   pixels UNIFONT_BLOB SCREENS OUT
 *
 * UNIFONT_BLOB is what genunifont writes from font_unifont_data.hex. A
 * screen with a second part is drawn as four frames into two buffers, as
 * test_pixels.c drew one, and fails unless both buffers end equal.
 */

#undef NDEBUG
#include <assert.h>
#include <errno.h>
#include <libtsm.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

const char *_binary_font_unifont_data_start;
size_t _binary_font_unifont_data_size;
#include "src/font/font.c"
#include "src/font/font_box.c"
#undef LOG_SUBSYSTEM
#include "src/font/font_unifont.c"
#undef LOG_SUBSYSTEM
#include "src/render/text.c"
#undef LOG_SUBSYSTEM
#include "src/render/bbulk.c"
#include "src/render/image_budget.c"
#include "src/render/picture.c"
#undef LOG_SUBSYSTEM
#include "src/render/deco.c"
#undef LOG_SUBSYSTEM
#include "src/video/drm2d_render.c"

void shl_module_ref(struct shl_module *module) {}
void shl_module_unref(struct shl_module *module) {}

/* the in-memory display test_pixels.c drew into */
static uint32_t *fb, *fb_back;
static unsigned int fb_w, fb_h;
static struct drm2d_display fake_d2d;
static struct display fake_disp;

unsigned int display_get_width(struct display *disp)
{
	return fb_w;
}
unsigned int display_get_height(struct display *disp)
{
	return fb_h;
}
void display_ref(struct display *disp) {}
void display_unref(struct display *disp) {}
bool display_need_redraw(struct display *disp)
{
	return false;
}
bool display_supports_damage(struct display *disp)
{
	return false;
}
void display_set_damage(struct display *disp, size_t n_rect, struct video_rect *damages) {}
int display_swap(struct display *disp)
{
	return 0;
}
bool display_need_commit(struct display *disp)
{
	return false;
}
void display_set_cursor_offset(struct display *disp, int32_t x, int32_t y) {}
int display_clear(struct display *disp, uint8_t r, uint8_t g, uint8_t b)
{
	return drm2d_display_clear(disp, r, g, b);
}
int display_blend(struct display *disp, const struct video_blend_req *req)
{
	return drm2d_display_blend(disp, req);
}

static void vte_write(struct tsm_vte *vte, const char *u8, size_t len, void *data) {}

static void draw_frame(struct kmscon_text *txt, struct tsm_screen *con, struct tsm_vte *vte)
{
	struct tsm_screen_attr attr;
	uint32_t *tmp;

	tsm_vte_get_def_attr(vte, &attr);
	assert(kmscon_text_prepare(txt, &attr, false) == 0);
	assert(kmscon_text_draw(txt, con, false) == 0);
	assert(kmscon_text_render(txt) == 0);
	tmp = fb;
	fb = fb_back;
	fb_back = tmp;
	fake_d2d.rb[1].map = fb;
}

/* @hex's bytes, written to @out of @size bytes; their count, or -1 when
 * @hex is not hex or does not fit */
static long unhex(const char *hex, char *out, size_t size)
{
	size_t n = strlen(hex);
	unsigned int v;

	if (n % 2 || n / 2 > size)
		return -1;
	for (size_t i = 0; i < n / 2; i++) {
		if (sscanf(hex + 2 * i, "%2x", &v) != 1)
			return -1;
		out[i] = (char)v;
	}
	return (long)(n / 2);
}

static int draw(struct kmscon_font *font, char **f, const char *out_dir)
{
	unsigned int cols = atoi(f[1]), rows = atoi(f[2]), w = atoi(f[3]), h = atoi(f[4]);
	enum Orientation orientation = !strcmp(f[5], "right")	      ? OR_RIGHT
				       : !strcmp(f[5], "upside-down") ? OR_UPSIDE_DOWN
				       : !strcmp(f[5], "left")	      ? OR_LEFT
								      : OR_NORMAL;
	static char bytes[1 << 16], next[1 << 16];
	long nbytes = unhex(f[7], bytes, sizeof(bytes));
	long nnext = strcmp(f[8], "-") ? unhex(f[8], next, sizeof(next)) : -1;
	struct tsm_screen *con;
	struct tsm_vte *vte;
	struct kmscon_text *txt;
	char path[4096];
	FILE *ppm;
	int ret = 0;

	if (nbytes < 0 || (strcmp(f[8], "-") && nnext < 0)) {
		fprintf(stderr, "pixels: %s: bad hex, or past %zu bytes\n", f[0], sizeof(bytes));
		return -EINVAL;
	}
	fb_w = w ? w : cols * font->width;
	fb_h = h ? h : rows * font->height;
	if (orientation == OR_RIGHT || orientation == OR_LEFT) {
		unsigned int t = fb_w;

		fb_w = fb_h;
		fb_h = t;
	}
	fb = calloc((size_t)fb_w * fb_h, sizeof(*fb));
	fb_back = calloc((size_t)fb_w * fb_h, sizeof(*fb));
	assert(fb && fb_back);
	fake_disp.width = fb_w;
	fake_disp.height = fb_h;
	fake_disp.data = &fake_d2d;
	fake_d2d.current_rb = 0;
	fake_d2d.rb[1].map = fb;
	fake_d2d.rb[1].stride = fb_w * 4;

	assert(tsm_screen_new(&con, NULL, NULL) == 0);
	assert(tsm_vte_new(&vte, con, vte_write, NULL, NULL, NULL) == 0);
	tsm_vte_set_palette(vte, "default");
	assert(tsm_vte_set_width_method(vte, strcmp(f[6], "wcwidth") ? TSM_WIDTH_UNICODE
								     : TSM_WIDTH_WCWIDTH) == 0);
	assert(tsm_screen_resize(con, cols, rows) == 0);
	assert(tsm_screen_set_cell_size(con, font->width, font->height) == 0);
	/* hidden cursor: its blink state is not part of what is compared */
	tsm_vte_input(vte, "\x1b[?25l", 6);
	tsm_vte_input(vte, bytes, nbytes);

	assert(kmscon_text_new(&txt, "bbulk", orientation, &fake_disp) == 0);
	assert(kmscon_text_set(txt, font) == 0);
	draw_frame(txt, con, vte);
	if (nnext >= 0) {
		draw_frame(txt, con, vte);
		tsm_vte_input(vte, next, nnext);
		draw_frame(txt, con, vte);
		draw_frame(txt, con, vte);
		if (memcmp(fb, fb_back, (size_t)fb_w * fb_h * sizeof(*fb))) {
			fprintf(stderr, "pixels: %s: the two buffers differ after the last frame\n",
				f[0]);
			ret = -EINVAL;
		}
	}

	/* the buffer drawn last */
	snprintf(path, sizeof(path), "%s/%s.ppm", out_dir, f[0]);
	if (!ret && (ppm = fopen(path, "wb"))) {
		fprintf(ppm, "P6\n%u %u\n255\n", fb_w, fb_h);
		for (size_t i = 0; i < (size_t)fb_w * fb_h; i++) {
			uint32_t p = fb_back[i];
			uint8_t rgb[3] = {p >> 16, p >> 8, p};

			fwrite(rgb, 1, 3, ppm);
		}
		ret = fclose(ppm) ? -errno : 0;
	} else if (!ret) {
		ret = -errno;
	}
	if (ret)
		fprintf(stderr, "pixels: %s: %s\n", f[0], strerror(-ret));
	else
		printf("%s\n", path);

	kmscon_text_unref(txt);
	tsm_vte_unref(vte);
	tsm_screen_unref(con);
	free(fb);
	free(fb_back);
	return ret;
}

int main(int argc, char **argv)
{
	static uint8_t blob[4 << 20];
	static char line[1 << 18];
	struct kmscon_font *font;
	FILE *f;
	int failed = 0;

	if (argc != 4 || !(f = fopen(argv[1], "rb"))) {
		fprintf(stderr, "usage: pixels UNIFONT_BLOB SCREENS OUT\n");
		return 2;
	}
	_binary_font_unifont_data_size = fread(blob, 1, sizeof(blob), f);
	_binary_font_unifont_data_start = (const char *)blob;
	if (_binary_font_unifont_data_size == sizeof(blob) && fgetc(f) != EOF) {
		fprintf(stderr, "pixels: %s is past %zu bytes\n", argv[1], sizeof(blob));
		return 2;
	}
	fclose(f);

	assert(kmscon_text_register(&kmscon_text_bbulk_ops) == 0);
	font = calloc(1, sizeof(*font));
	assert(font);
	font->ref = 1;
	font->ops = &kmscon_font_unifont_ops;
	if (font->ops->init(font, "", 16)) {
		fprintf(stderr, "pixels: unifont failed to load\n");
		return 2;
	}

	if (!(f = fopen(argv[2], "r"))) {
		fprintf(stderr, "pixels: cannot read %s\n", argv[2]);
		return 2;
	}
	while (fgets(line, sizeof(line), f)) {
		char *fields[9], *save = NULL;
		int n = 0;

		line[strcspn(line, "\n")] = 0;
		for (char *t = strtok_r(line, "\t", &save); t && n < 9; t = strtok_r(NULL, "\t", &save))
			fields[n++] = t;
		if (n != 9) {
			fprintf(stderr, "pixels: a line of %d fields, want 9\n", n);
			return 2;
		}
		failed |= !!draw(font, fields, argv[3]);
	}
	fclose(f);
	return failed;
}
