/*
 * vtium-font - digests of the C reference's glyphs, for tests/reference.rs
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
 * Prints FNV-1a 64 digests of the glyphs the C reference draws, one line
 * each, in the format tests/reference.rs reads. Built three times against
 * the C reference's sources, once per backend (see README.md):
 *
 *   -DREF_BOX      font_box.c: the sprites, swept over cell sizes
 *   -DREF_PSF      font_psf.c and font.c: each fixture font, raw and drawn
 *                  through kmscon_font_render; argv names the fonts
 *   -DREF_UNIFONT  font_unifont.c and font.c: the built-in unifont, raw and
 *                  drawn through kmscon_font_render; argv[1] is the blob
 *                  genunifont writes
 *
 * A digest covers each glyph's width and height (little-endian u32) and
 * then its pixels, row by row, over the glyphs of a line in order.
 */

#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#ifdef REF_BOX
#include "font/font_box.c"
#endif

#ifdef REF_PSF
#include "font/font.c"
#include "font/font_box.c"
#include "font/font_psf.c"
#define BACKEND kmscon_font_psf_ops
#endif

#ifdef REF_UNIFONT
const char *_binary_font_unifont_data_start;
size_t _binary_font_unifont_data_size;
#include "font/font.c"
#include "font/font_box.c"
#include "font/font_unifont.c"
#define BACKEND kmscon_font_unifont_ops
#endif

#if defined(REF_PSF) || defined(REF_UNIFONT)
/* font.c's registry and the logger are not needed: the backend is set
 * directly and nothing is logged */
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
#endif

#define FNV_OFFSET 0xcbf29ce484222325ULL
#define FNV_PRIME 0x100000001b3ULL

static uint64_t fnv(uint64_t h, const uint8_t *p, size_t n)
{
	for (size_t i = 0; i < n; i++)
		h = (h ^ p[i]) * FNV_PRIME;
	return h;
}

static uint64_t digest(uint64_t h, unsigned int w, unsigned int ht, const uint8_t *px)
{
	uint8_t le[8] = {w, w >> 8, w >> 16, w >> 24, ht, ht >> 8, ht >> 16, ht >> 24};

	h = fnv(h, le, 8);
	return fnv(h, px, (size_t)w * ht);
}

static uint64_t digest_glyph(uint64_t h, struct kmscon_glyph *g)
{
	h = digest(h, g->buf.width, g->buf.height, g->buf.data);
	free(g);
	return h;
}

#ifdef REF_BOX
static const struct {
	uint32_t first, last;
} ranges[] = {
	{0x2500, 0x257f}, {0x2580, 0x259f}, {0x2800, 0x28ff}, {0x1fb00, 0x1fb3b}, {0x1cd00, 0x1cde5},
};

/* font.c's kmscon_font_get_deco() for a backend with no metrics */
static unsigned int deco_thick(unsigned int h)
{
	unsigned int t = h / 16, max = h / 4 ? h / 4 : 1;

	return t < 1 ? 1 : t > max ? max : t;
}

static void box_line(size_t r, unsigned int w, unsigned int h, unsigned int t)
{
	uint64_t d = FNV_OFFSET;

	for (uint32_t cp = ranges[r].first; cp <= ranges[r].last; cp++)
		d = digest_glyph(d, kmscon_font_box_render(cp, w, h, t));
	printf("box\t%x-%x\t%u\t%u\t%u\t%016llx\n", ranges[r].first, ranges[r].last, w, h, t,
	       (unsigned long long)d);
}

int main(void)
{
	static const unsigned int ws[] = {1, 2, 3, 5, 8, 9, 10, 12, 16, 17, 20, 24, 32};
	static const unsigned int hs[] = {1, 2, 3, 8, 16, 19, 28, 32, 33, 40, 64};
	static const unsigned int ts[] = {1, 2, 3, 4, 5, 8};

	for (unsigned int h = 1; h <= 40; h++)
		for (unsigned int w = 1; w <= 20; w++)
			for (size_t r = 0; r < sizeof(ranges) / sizeof(ranges[0]); r++)
				box_line(r, w, h, deco_thick(h));
	for (size_t i = 0; i < sizeof(ws) / sizeof(ws[0]); i++)
		for (size_t j = 0; j < sizeof(hs) / sizeof(hs[0]); j++)
			for (size_t k = 0; k < sizeof(ts) / sizeof(ts[0]); k++)
				box_line(0, ws[i], hs[j], ts[k]);
	for (size_t r = 1; r < sizeof(ranges) / sizeof(ranges[0]); r++)
		box_line(r, 1000, 2000, 1);
	return 0;
}
#endif

#if defined(REF_PSF) || defined(REF_UNIFONT)
/* the 256 codepoints of @page drawn through kmscon_font_render */
static void render_page(const char *name, struct kmscon_font *font, unsigned int height,
			uint32_t page, unsigned int cwidth, int bold)
{
	struct kmscon_font_attr attr = {.bold = bold};
	uint64_t d = FNV_OFFSET;

	for (uint32_t cp = page << 8; cp < (page + 1) << 8; cp++) {
		struct kmscon_glyph *g = kmscon_font_render(font, &attr, &cp, 1, cwidth);

		if (!g) {
			fprintf(stderr, "refdigest: U+%04X drew nothing\n", cp);
			exit(1);
		}
		d = digest_glyph(d, g);
	}
	printf("render\t%s\t%u\t%x\t%u\t%d\t%016llx\n", name, height, page, cwidth, bold,
	       (unsigned long long)d);
}

/* the pages render_page draws: each of plane 0 when @all, else a sample */
static void render_pages(const char *name, struct kmscon_font *font, unsigned int height, int all)
{
	static const uint32_t sample[] = {0x00, 0x01, 0x03, 0x04, 0x20, 0x22,
					  0x25, 0x28, 0x4e, 0xff, 0x1cd, 0x1fb};
	static const uint32_t extra[] = {0x1cd, 0x1f6, 0x1fb, 0x200, 0x10ff};

	if (all) {
		for (uint32_t page = 0; page < 0x100; page++)
			for (unsigned int cw = 1; cw <= 2; cw++)
				render_page(name, font, height, page, cw, 0);
		for (size_t i = 0; i < sizeof(extra) / sizeof(extra[0]); i++)
			for (unsigned int cw = 1; cw <= 2; cw++)
				render_page(name, font, height, extra[i], cw, 0);
	} else {
		for (size_t i = 0; i < sizeof(sample) / sizeof(sample[0]); i++)
			for (unsigned int cw = 1; cw <= 2; cw++)
				render_page(name, font, height, sample[i], cw, 0);
	}
	for (size_t i = 0; i < 9; i++)
		render_page(name, font, height, (uint32_t[]){0, 1, 3, 4, 0x20, 0x25, 0x28, 0x4e,
							     0x1fb}[i],
			    1, 1);
}

static void load(struct kmscon_font *font, const char *path, unsigned int height)
{
	memset(font, 0, sizeof(*font));
	font->ref = 1;
	font->ops = &BACKEND;
	if (font->ops->init(font, path, height)) {
		fprintf(stderr, "refdigest: %s at %u failed to load\n", path, height);
		exit(1);
	}
}
#endif

#ifdef REF_PSF
int main(int argc, char **argv)
{
	for (int i = 1; i < argc; i++) {
		const char *name = strrchr(argv[i], '/') ? strrchr(argv[i], '/') + 1 : argv[i];

		for (unsigned int height = 16; height <= 48; height += 16) {
			struct kmscon_font font;
			struct kmscon_font_attr attr = {0};
			psf_font_t *psf;

			load(&font, argv[i], height);
			psf = font.data;
			for (int bold = 0; bold < 2; bold++) {
				uint64_t d = FNV_OFFSET;

				attr.bold = bold;
				for (uint32_t g = 0; g < psf->glyphs; g++)
					d = digest_glyph(d, new_glyph(&font, &attr, g));
				printf("psf-glyphs\t%s\t%u\t%d\t%016llx\n", name, height, bold,
				       (unsigned long long)d);
			}
			render_pages(name, &font, height, 0);
			font.ops->destroy(&font);
		}
	}
	return 0;
}
#endif

#ifdef REF_UNIFONT
int main(int argc, char **argv)
{
	static uint8_t blob[4 << 20];
	struct kmscon_font font;
	FILE *f;

	if (argc != 2 || !(f = fopen(argv[1], "rb"))) {
		fprintf(stderr, "refdigest: name the blob genunifont writes\n");
		return 1;
	}
	_binary_font_unifont_data_size = fread(blob, 1, sizeof(blob), f);
	_binary_font_unifont_data_start = (const char *)blob;
	fclose(f);

	for (unsigned int height = 16; height <= 32; height += 16) {
		struct unifont_data *uf;
		const struct unifont_glyph_block *blocks;
		uint32_t nblocks;

		load(&font, "", height);
		uf = font.data;
		nblocks = *(uint32_t *)uf->font_data;
		blocks = (const struct unifont_glyph_block *)(uf->font_data + 4);
		for (int bold = 0; bold < 2; bold++) {
			struct kmscon_font_attr attr = {.bold = bold};
			uint32_t page = UINT32_MAX;
			uint64_t d = FNV_OFFSET;

			if (bold && height != 16)
				break;
			/* each glyph unifont holds at its own width, a digest a page */
			for (uint32_t b = 0; b < nblocks; b++) {
				for (uint32_t cp = blocks[b].codepoint;
				     cp < blocks[b].codepoint + blocks[b].len; cp++) {
					if (cp >> 8 != page) {
						if (page != UINT32_MAX)
							printf("unifont-glyphs\t%u\t%d\t%x\t%016llx\n",
							       height, bold, page,
							       (unsigned long long)d);
						page = cp >> 8;
						d = FNV_OFFSET;
					}
					d = digest_glyph(d, kmscon_font_unifont_render(
								    &font, &attr, &cp, 1, 1));
				}
			}
			printf("unifont-glyphs\t%u\t%d\t%x\t%016llx\n", height, bold, page,
			       (unsigned long long)d);
		}
		render_pages("unifont", &font, height, height == 16);
		font.ops->destroy(&font);
	}
	return 0;
}
#endif
