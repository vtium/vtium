/*
 * vtium-render - digests of whole cells drawn with the C reference's
 * arithmetic, glyphs and decoration sprites, for tests/reference.rs
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
 * Prints FNV-1a 64 digests in the format tests/reference.rs reads:
 *
 *   deco <kind> <w> <h> <digest>   kmscon_deco_render()'s sprite of each
 *                                  kind at each cell size, with the lines
 *                                  kmscon_font_get_deco() derives for it
 *   screen <name> <digest>         each screen of screens.tsv, every cell
 *                                  drawn as bbulk_draw_cell() drew it on an
 *                                  unturned display without pictures:
 *                                  its glyph from kmscon_font_render()
 *                                  through unifont, decorated cells composed
 *                                  with bbulk.c's compose() and every cell
 *                                  blended with drm2d_render.c's
 *                                  blend_mask() and blend_argb()
 *
 * blend.inc holds those three functions, cut from the C reference's
 * sources by the recipe in README.md, so the arithmetic is the C
 * reference's own. A screen digest covers its width and height as
 * little-endian u32 and then its 0x00RRGGBB pixels, little-endian, row by
 * row.
 */

#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

const char *_binary_font_unifont_data_start;
size_t _binary_font_unifont_data_size;
#include "font/font.c"
#include "font/font_box.c"
#include "font/font_unifont.c"
#include "render/deco.c"
#include "video/video.h"

void shl_module_ref(struct shl_module *module)
{
	(void)module;
}
void shl_module_unref(struct shl_module *module)
{
	(void)module;
}
void log_submit(enum log_severity level, const char *subs, const char *format, va_list args)
{
	(void)level, (void)subs, (void)format, (void)args;
}
void log_format(enum log_severity level, const char *subs, const char *format, ...)
{
	(void)level, (void)subs, (void)format;
}
void log_llog(void *data, const char *file, int line, const char *func, const char *subs,
	      unsigned int sev, const char *format, va_list args)
{
	(void)data, (void)file, (void)line, (void)func, (void)subs, (void)sev, (void)format,
		(void)args;
}

#include "blend.inc"

#define FNV_OFFSET 0xcbf29ce484222325ULL
#define FNV_PRIME 0x100000001b3ULL

static uint64_t fnv(uint64_t h, const uint8_t *p, size_t n)
{
	for (size_t i = 0; i < n; i++)
		h = (h ^ p[i]) * FNV_PRIME;
	return h;
}

static uint64_t fnv_u32(uint64_t h, uint32_t v)
{
	uint8_t le[4] = {v, v >> 8, v >> 16, v >> 24};

	return fnv(h, le, 4);
}

/* font.c's kmscon_font_get_deco() for a backend without metrics */
static void deco_for(unsigned int h, struct kmscon_font_deco *m)
{
	struct kmscon_font_ops ops = {0};
	struct kmscon_font font = {.ops = &ops, .height = h};

	kmscon_font_get_deco(&font, m);
}

static void decos(void)
{
	for (unsigned int h = 1; h <= 48; h++) {
		struct kmscon_font_deco m;

		deco_for(h, &m);
		for (unsigned int w = 1; w <= 24; w++) {
			for (int kind = KMSCON_DECO_SINGLE; kind < KMSCON_DECO_COUNT; kind++) {
				struct kmscon_glyph *g = kmscon_deco_render(kind, w, h, &m);
				uint64_t d = FNV_OFFSET;

				d = fnv_u32(d, w);
				d = fnv_u32(d, h);
				d = fnv(d, g->buf.data, (size_t)w * h);
				printf("deco\t%d\t%u\t%u\t%016llx\n", kind, w, h,
				       (unsigned long long)d);
				free(g);
			}
		}
	}
}

struct cell {
	uint32_t cp;
	unsigned int cells;
	struct tsm_screen_color fg, bg, ul;
	int bold, underline, strike, ul_set;
};

static struct tsm_screen_color rgb(const char *hex)
{
	unsigned long v = strtoul(hex, NULL, 16);

	return (struct tsm_screen_color){.r = v >> 16, .g = v >> 8, .b = v};
}

/* drm2d_display_blend() into a plain buffer: @req's buffer over its
 * colours, clipped to the request and the buffer */
static void blend(uint32_t *fb, unsigned int fbw, const struct video_blend_req *req)
{
	unsigned int w = req->w < req->buf->width ? req->w : req->buf->width;
	unsigned int h = req->h < req->buf->height ? req->h : req->buf->height;
	int argb = req->buf->format == VIDEO_FORMAT_ARGB32;

	for (unsigned int y = 0; y < h; y++) {
		uint32_t *dst = &fb[(req->y + y) * fbw + req->x];

		for (unsigned int x = 0; x < w; x++) {
			if (argb)
				dst[x] = blend_argb(
					req, ((const uint32_t *)req->buf->data)[y * req->buf->width + x]);
			else
				dst[x] = blend_mask(req, req->buf->data[y * req->buf->width + x]);
		}
	}
}

static struct kmscon_font font;
static struct kmscon_glyph *sprites[KMSCON_DECO_COUNT];
static struct video_buffer *scratch;
/* bbulk's blank glyph: two cells of zeros */
static struct video_buffer *blank;

/* bbulk_draw_cell() on an unturned display with no picture over the cell */
static void draw_cell(uint32_t *fb, unsigned int fbw, const struct cell *c, unsigned int posx,
		      unsigned int posy, unsigned int ncells)
{
	unsigned int fw = font.width, fh = font.height;
	struct kmscon_font_attr attr = {.bold = c->bold};
	struct kmscon_glyph *glyph = NULL;
	struct video_blend_req req = {0};
	struct tsm_screen_color ulc = c->ul_set ? c->ul : c->fg;

	if (c->cp) {
		glyph = kmscon_font_render(&font, &attr, &c->cp, 1, c->cells);
		if (!glyph) {
			fprintf(stderr, "refcells: U+%04X drew nothing\n", c->cp);
			exit(1);
		}
	}
	req.x = posx * fw;
	req.y = posy * fh;
	req.w = ncells * fw;
	req.h = fh;
	req.fr = c->fg.r, req.fg = c->fg.g, req.fb = c->fg.b;
	req.br = c->bg.r, req.bg = c->bg.g, req.bb = c->bg.b;
	if (!c->underline && !c->strike) {
		req.buf = glyph ? &glyph->buf : blank;
	} else {
		const struct video_buffer *gb = glyph ? &glyph->buf : blank;

		scratch->width = req.w < gb->width ? req.w : gb->width;
		scratch->height = req.h < gb->height ? req.h : gb->height;
		scratch->format = VIDEO_FORMAT_ARGB32;
		memset(scratch->data, 0, scratch->width * scratch->height * 4);
		if (c->underline)
			for (unsigned int i = 0; i < ncells; i++)
				compose(scratch, &sprites[c->underline]->buf, i * fw, 0, ulc);
		if (glyph)
			compose(scratch, &glyph->buf, 0, 0, c->fg);
		if (c->strike)
			for (unsigned int i = 0; i < ncells; i++)
				compose(scratch, &sprites[KMSCON_DECO_STRIKE]->buf, i * fw, 0, c->fg);
		req.buf = scratch;
	}
	blend(fb, fbw, &req);
	free(glyph);
}

static void flush(const char *name, unsigned int cols, unsigned int rows, struct cell *cells)
{
	unsigned int fbw = cols * font.width, fbh = rows * font.height;
	uint32_t *fb = calloc((size_t)fbw * fbh, 4);
	uint64_t d = FNV_OFFSET;

	for (unsigned int y = 0; y < rows; y++) {
		for (unsigned int x = 0; x < cols; x++) {
			struct cell *c = &cells[y * cols + x];

			/* a trailing half is drawn by the wide cell before it */
			if (!c->cells)
				continue;
			draw_cell(fb, fbw, c, x, y, c->cells == 2 && x + 1 < cols ? 2 : 1);
		}
	}
	d = fnv_u32(d, fbw);
	d = fnv_u32(d, fbh);
	for (size_t i = 0; i < (size_t)fbw * fbh; i++)
		d = fnv_u32(d, fb[i]);
	printf("screen\t%s\t%016llx\n", name, (unsigned long long)d);
	free(fb);
}

static void screens(const char *path)
{
	FILE *f = fopen(path, "r");
	char line[512], name[64] = "";
	unsigned int cols = 0, rows = 0;
	struct cell *cells = NULL;
	struct tsm_screen_color dfg = {0}, dbg = {0};

	if (!f) {
		fprintf(stderr, "refcells: cannot read %s\n", path);
		exit(1);
	}
	while (fgets(line, sizeof(line), f)) {
		char kind[16], a[16], b[16], ul[16];
		unsigned int x, y, n, cp;
		int bold, under, strike;

		if (line[0] == '#')
			continue;
		if (!strncmp(line, "screen", 6)) {
			if (cells)
				flush(name, cols, rows, cells);
			free(cells);
			cells = NULL;
			if (sscanf(line, "screen %63s %u %u %15s %15s", name, &cols, &rows, a, b) != 5) {
				fprintf(stderr, "refcells: bad line: %s", line);
				exit(1);
			}
			dfg = rgb(a);
			dbg = rgb(b);
			cells = calloc((size_t)cols * rows, sizeof(*cells));
			for (size_t i = 0; i < (size_t)cols * rows; i++)
				cells[i] = (struct cell){.cells = 1, .fg = dfg, .bg = dbg};
			continue;
		}
		if (sscanf(line, "%15s %u %u %x %u %15s %15s %d %d %d %15s", kind, &x, &y, &cp, &n, a,
			   b, &bold, &under, &strike, ul) == 11 &&
		    !strcmp(kind, "cell")) {
			struct cell *c = &cells[y * cols + x];

			*c = (struct cell){.cp = cp, .cells = n, .fg = rgb(a), .bg = rgb(b),
					   .bold = bold, .underline = under, .strike = strike,
					   .ul_set = strcmp(ul, "-") != 0};
			if (c->ul_set)
				c->ul = rgb(ul);
			if (n == 2 && x + 1 < cols)
				cells[y * cols + x + 1] = (struct cell){.cells = 0};
			continue;
		}
	}
	fclose(f);
	if (cells)
		flush(name, cols, rows, cells);
	free(cells);
}

int main(int argc, char **argv)
{
	static uint8_t blob[4 << 20];
	struct kmscon_font_deco m;
	FILE *f;

	if (argc < 3 || !(f = fopen(argv[1], "rb"))) {
		fprintf(stderr, "refcells: name the blob genunifont writes, then screens\n");
		return 1;
	}
	_binary_font_unifont_data_size = fread(blob, 1, sizeof(blob), f);
	_binary_font_unifont_data_start = (const char *)blob;
	fclose(f);

	decos();

	font.ref = 1;
	font.ops = &kmscon_font_unifont_ops;
	if (font.ops->init(&font, "", 16)) {
		fprintf(stderr, "refcells: unifont failed to load\n");
		return 1;
	}
	kmscon_font_get_deco(&font, &m);
	for (int kind = KMSCON_DECO_SINGLE; kind < KMSCON_DECO_COUNT; kind++)
		sprites[kind] = kmscon_deco_render(kind, font.width, font.height, &m);
	scratch = calloc(1, sizeof(*scratch) + 2 * font.width * font.height * 4);
	blank = calloc(1, sizeof(*blank) + 2 * font.width * font.height);
	blank->width = 2 * font.width;
	blank->height = font.height;
	blank->format = VIDEO_FORMAT_A8;
	for (int i = 2; i < argc; i++)
		screens(argv[i]);
	return 0;
}
