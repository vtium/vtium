/*
 * Tier 2 conformance test (ADR-301): render fixed screens through the real
 * text renderer and font backend into an in-memory XRGB8888 framebuffer and
 * write each one as a PPM (P6) for pixcmp to compare with its golden.
 *
 * Usage: test_pixels FONT_DIR OUT_DIR
 *
 * Fonts are loaded from FONT_DIR through a private fontconfig configuration
 * that holds nothing else, so the system's fonts and fontconfig rules cannot
 * change the output.
 *
 * The pipeline is the production one: libtsm parses bytes into a screen,
 * kmscon_text_draw() reads it, the bbulk renderer asks the freetype backend
 * for glyph masks and hands them to display_blend(). Only the display is
 * replaced, by a fake struct display that the real drm2d blend and clear
 * draw into.
 */

#undef NDEBUG
#include <assert.h>
#include <errno.h>
#include <fontconfig/fontconfig.h>
#include <libtsm.h>
#include <limits.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "../src/font/font.c"
#include "../src/font/font_box.c"
#undef LOG_SUBSYSTEM
#include "../src/font/font_freetype.c"
#undef LOG_SUBSYSTEM
#include "../src/font/font_colour.c"
#undef LOG_SUBSYSTEM
#include "../src/render/text.c"
#undef LOG_SUBSYSTEM
#include "../src/render/bbulk.c"
#include "../src/render/image_budget.c"
#include "../src/render/picture.c"
#undef LOG_SUBSYSTEM
#include "../src/render/deco.c"
#undef LOG_SUBSYSTEM
#include "../src/video/drm2d_render.c"
#include "box_model.h"
#include "config.h"

void shl_module_ref(struct shl_module *module) {}
void shl_module_unref(struct shl_module *module) {}

/* ---- in-memory display ---- */

static uint32_t *fb, *fb_back;
static unsigned int fb_w, fb_h;

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

/*
 * The real drm2d blend and clear run against a fake display whose back
 * buffer is fb: display_blend() and display_clear() forward to
 * drm2d_display_blend() and drm2d_display_clear() from
 * src/video/drm2d_render.c, which is compiled into this test.
 */
static struct drm2d_display fake_d2d;
static struct display fake_disp;

static void fake_display_init(void)
{
	fake_disp.width = fb_w;
	fake_disp.height = fb_h;
	fake_disp.data = &fake_d2d;
	fake_d2d.current_rb = 0;
	fake_d2d.rb[1].map = fb;
	fake_d2d.rb[1].stride = fb_w * 4;
}

int display_clear(struct display *disp, uint8_t r, uint8_t g, uint8_t b)
{
	return drm2d_display_clear(disp, r, g, b);
}

int display_blend(struct display *disp, const struct video_blend_req *req)
{
	return drm2d_display_blend(disp, req);
}

/* ---- screens ---- */

#define COLS 48
#define ROWS 10
#define FONT_PX 16
#define ESC "\x1b"

/*
 * A structural check of the drawn screen, over a rectangle of cells, against
 * the background (the screen's last pixel, which no screen draws on):
 * 'f' every pixel the same colour and not the background (blocks that meet);
 * 'v' every pixel row holds a pixel that is not the background (a line that
 *     runs down from row to row);
 * 'h' the same for every pixel column (a line that runs across).
 * The emoji checks judge a glyph's box, one or two cells, against its own
 * background, the colour of its top-left pixel:
 * 'b' the glyph is inside its box: the box's outermost ring of pixels is all
 *     background and something is drawn inside it, so no ink was cut at an
 *     edge (ADR-102 step 4c: an emoji is fitted to its cells, not clipped);
 * 'c' the box holds colour: a pixel whose red, green and blue differ;
 * 'm' the box holds no colour: every pixel grey, as a mask in the grey
 *     default foreground draws it.
 * The mosaic check judges Braille, sextants and octants (#76):
 * 'p' each cell holds the next codepoint of @cells, row by row, and is
 *     box_model.h's sprite of it exactly: every pixel the sprite covers in
 *     one colour, every other pixel in another, the same two colours across
 *     the rectangle. A cell's blocks reach its edges exactly, so blocks in
 *     neighbouring cells meet with nothing between them.
 */
struct check {
	char kind;
	unsigned int col, row, cols, rows;
	/* 'p': the cells' codepoints, UTF-8 */
	const char *cells;
};

struct screen {
	const char *name;
	/* what the screen shows and what correct looks like: the caption on
	 * the conformance run page, written beside the image as NAME.txt */
	const char *desc;
	const char *bytes;
	/* optional: the bytes of a second frame; frames 2 and 4 repeat the
	 * one before, so the result is the 4th frame's buffer */
	const char *next;
	/* optional: the decoration settings of kmscon.conf */
	const struct kmscon_conf_t *conf;
	/* optional: the font's pixel size, FONT_PX if 0 */
	unsigned int px;
	/* optional: pixel rows added to the font's line height, standing in for
	 * a font whose line height has line spacing beyond its glyphs (#46) */
	unsigned int spacing;
	/* optional: checks, ended by kind 0 */
	const struct check *checks;
	/* optional: kmscon.conf's no-emoji-color */
	bool no_emoji_color;
	/* optional: the screen's size in cells, COLS x ROWS if 0 */
	unsigned int cols, rows;
	/*
	 * optional: pictures and placements (ADR-103), made through libtsm's
	 * test calls or sent as kitty commands to @vte, before frame @frame is
	 * drawn: frame 0 only, or 0 to 3 for a screen with @next. @cw x @ch is
	 * the font's cell, which the screen measures placements in.
	 */
	void (*images)(struct tsm_screen *con, struct tsm_vte *vte, unsigned int frame,
		       unsigned int cw, unsigned int ch);
	/* optional: the display's orientation, OR_NORMAL if 0 */
	enum Orientation orientation;
	/* optional: the render timer's on-screen display from frame 0, and
	 * from frame 2 of a screen with @next */
	const char *osd, *osd_next;
};

/* the ADR-102 decoration settings the deco-* screens configure */
static const struct kmscon_conf_t conf_deco = {
	.underline_color = {.kind = KMSCON_COLOR_INDEX, .index = 196},
	.strike_color = {.kind = KMSCON_COLOR_RGB, .rgb = {0x00, 0xc0, 0xff}},
};
static const struct kmscon_conf_t conf_deco_force = {
	.underline_color = {.kind = KMSCON_COLOR_INDEX, .index = 196},
	.strike_color = {.kind = KMSCON_COLOR_RGB, .rgb = {0x00, 0xc0, 0xff}},
	.force_decoration_colors = true,
};

/* UTF-8 */
#define E_ACUTE "\xc3\xa9"
#define A_TILDE "\xc3\xa3"
#define N_TILDE "\xc3\xb1"
#define COMB_GRAVE "\xcc\x80"
#define COMB_ACUTE "\xcc\x81"
#define COMB_TILDE "\xcc\x83"
#define COMB_DIAER "\xcc\x88"
#define HEART "\xe2\x9d\xa4"
#define VS15 "\xef\xb8\x8e"
#define VS16 "\xef\xb8\x8f"
#define ZWJ "\xe2\x80\x8d"
#define KEYCAP "\xe2\x83\xa3"
#define GRIN "\xf0\x9f\x98\x80"	   /* U+1F600 */
#define THUMB "\xf0\x9f\x91\x8d"   /* U+1F44D */
#define TONE3 "\xf0\x9f\x8f\xbd"   /* U+1F3FD, medium skin tone */
#define MAN "\xf0\x9f\x91\xa8"	   /* U+1F468 */
#define WOMAN "\xf0\x9f\x91\xa9"   /* U+1F469 */
#define GIRL "\xf0\x9f\x91\xa7"	   /* U+1F467 */
#define RI_J "\xf0\x9f\x87\xaf"	   /* U+1F1EF, regional indicator J */
#define RI_P "\xf0\x9f\x87\xb5"	   /* U+1F1F5, regional indicator P */
#define SMILE "\xe2\x98\xba"	   /* U+263A, text presentation */
#define SHAKING "\xf0\x9f\xab\xa8" /* U+1FAE8, in no pinned font */

#define DECO_CONFIG_BYTES                                                                          \
	ESC "[4mconfigured underline" ESC "[0m " ESC "[9mconfigured strike" ESC "[0m\r\n" ESC      \
	    "[4:3;58:5:46mSGR 58 green" ESC "[0m " ESC "[4:4;58:2::255:255:0mSGR 58 yellow" ESC    \
	    "[0m\r\n" ESC "[32;4mgreen text" ESC "[0m " ESC "[33;9mstruck yellow text" ESC         \
	    "[0m\r\n" ESC "[4:2;9mdouble and strike" ESC "[0m\r\n"

/*
 * Blocks and box lines stacked across rows and columns (#46): full blocks,
 * halves that meet across a row and side by side, quadrants, shades, and
 * light, double, heavy, rounded, diagonal and dashed lines whose boxes span
 * five rows.
 */
#define BLOCKS_BYTES                                                                               \
	"██████ ▄▄▄▄ ┌──┬──┐ ╔══╦══╗ ┏━━┳━━┓ ╭──╮\r\n"                                             \
	"██████ ████ │  │  │ ║  ║  ║ ┃  ┃  ┃ │  │\r\n"                                             \
	"██████ ▀▀▀▀ ├──┼──┤ ╠══╬══╣ ┣━━╋━━┫ ╰──╯\r\n"                                             \
	"▐▌▐▌▐▌ ▖▗▘▝ │  │  │ ║  ║  ║ ┃  ┃  ┃ ╱╲╳\r\n"                                              \
	"▐▌▐▌▐▌ ▙▛▜▟ └──┴──┘ ╚══╩══╝ ┗━━┻━━┛ ▚▞\r\n"                                               \
	"▁▂▃▄▅▆▇█ ▏▎▍▌▋▊▉█ ▔▕ ░▒▓█ ┄┈╌ ┅┉╍\r\n"                                                    \
	"┆┊╎ ┇┋╏ ╞╡╟╢ ╒╕╓╖ ┍┑┎┒ ┝┥┠┨ ╼╽╾╿ ╴╵╶╷\r\n"                                                \
	"┆┊╎ ┇┋╏ ╘╛╙╜ ╪╫┿╂ ┕┙┖┚ ╀╁╃╄ ╅╆╇╈ ╉╊\r\n" ESC "[33;44m██▀▄▌▐░▒▓ ┼╋╬ ╭╮╰╯" ESC "[0m\r\n"

/* the boxes' corners and ends reach only one way, so the line checks start
 * past them; the double box's centre cross is four corners with nothing
 * between them */
static const struct check blocks_checks[] = {
	{'f', 0, 0, 6, 3},  /* full blocks */
	{'f', 7, 1, 4, 1},  /* full blocks between lower and upper halves */
	{'v', 12, 1, 1, 3}, /* light box: left, middle and right */
	{'v', 15, 1, 1, 3}, /* ... */
	{'v', 18, 1, 1, 3}, /* ... */
	{'v', 20, 1, 1, 3}, /* double box: left and right */
	{'v', 26, 1, 1, 3}, /* ... */
	{'v', 28, 1, 1, 3}, /* heavy box: left, middle and right */
	{'v', 31, 1, 1, 3}, /* ... */
	{'v', 34, 1, 1, 3}, /* ... */
	{'h', 13, 0, 5, 1}, /* light box: top, middle and bottom */
	{'h', 13, 2, 5, 1}, /* ... */
	{'h', 13, 4, 5, 1}, /* ... */
	{'h', 21, 0, 5, 1}, /* double box: top, middle either side, bottom */
	{'h', 21, 2, 2, 1}, /* ... */
	{'h', 24, 2, 2, 1}, /* ... */
	{'h', 21, 4, 5, 1}, /* ... */
	{'h', 29, 0, 5, 1}, /* heavy box: top, middle and bottom */
	{'h', 29, 2, 5, 1}, /* ... */
	{'h', 29, 4, 5, 1}, /* ... */
	{0},
};

#define MOSAIC_BRAILLE_0 "⠀⠁⠂⠃⠄⠅⠆⠇⠈⠉⠊⠋⠌⠍⠎⠏⠐⠑⠒⠓⠔⠕⠖⠗⠘⠙⠚⠛⠜⠝⠞⠟⠠⠡⠢⠣⠤⠥⠦⠧⠨⠩⠪⠫⠬⠭⠮⠯⠰⠱⠲⠳⠴⠵⠶⠷⠸⠹⠺⠻⠼⠽⠾⠿"
#define MOSAIC_BRAILLE_1 "⡀⡁⡂⡃⡄⡅⡆⡇⡈⡉⡊⡋⡌⡍⡎⡏⡐⡑⡒⡓⡔⡕⡖⡗⡘⡙⡚⡛⡜⡝⡞⡟⡠⡡⡢⡣⡤⡥⡦⡧⡨⡩⡪⡫⡬⡭⡮⡯⡰⡱⡲⡳⡴⡵⡶⡷⡸⡹⡺⡻⡼⡽⡾⡿"
#define MOSAIC_BRAILLE_2 "⢀⢁⢂⢃⢄⢅⢆⢇⢈⢉⢊⢋⢌⢍⢎⢏⢐⢑⢒⢓⢔⢕⢖⢗⢘⢙⢚⢛⢜⢝⢞⢟⢠⢡⢢⢣⢤⢥⢦⢧⢨⢩⢪⢫⢬⢭⢮⢯⢰⢱⢲⢳⢴⢵⢶⢷⢸⢹⢺⢻⢼⢽⢾⢿"
#define MOSAIC_BRAILLE_3 "⣀⣁⣂⣃⣄⣅⣆⣇⣈⣉⣊⣋⣌⣍⣎⣏⣐⣑⣒⣓⣔⣕⣖⣗⣘⣙⣚⣛⣜⣝⣞⣟⣠⣡⣢⣣⣤⣥⣦⣧⣨⣩⣪⣫⣬⣭⣮⣯⣰⣱⣲⣳⣴⣵⣶⣷⣸⣹⣺⣻⣼⣽⣾⣿"
#define MOSAIC_SEXTANTS "🬀🬁🬂🬃🬄🬅🬆🬇🬈🬉🬊🬋🬌🬍🬎🬏🬐🬑🬒🬓🬔🬕🬖🬗🬘🬙🬚🬛🬜🬝🬞🬟🬠🬡🬢🬣🬤🬥🬦🬧🬨🬩🬪🬫🬬🬭🬮🬯🬰🬱🬲🬳🬴🬵🬶🬷🬸🬹🬺🬻"
#define MOSAIC_OCTANTS_0                                                                           \
	"𜴀𜴁𜴂𜴃𜴄𜴅𜴆𜴇𜴈𜴉𜴊𜴋𜴌𜴍𜴎𜴏𜴐𜴑𜴒𜴓𜴔𜴕" \
	"𜴖𜴗𜴘𜴙𜴚𜴛𜴜𜴝𜴞𜴟𜴠𜴡𜴢𜴣𜴤𜴥𜴦𜴧𜴨𜴩𜴪𜴫" \
	"𜴬𜴭𜴮𜴯𜴰𜴱𜴲𜴳𜴴𜴵𜴶𜴷𜴸𜴹𜴺𜴻𜴼𜴽𜴾𜴿"
#define MOSAIC_OCTANTS_1                                                                           \
	"𜵀𜵁𜵂𜵃𜵄𜵅𜵆𜵇𜵈𜵉𜵊𜵋𜵌𜵍𜵎𜵏𜵐𜵑𜵒𜵓𜵔𜵕" \
	"𜵖𜵗𜵘𜵙𜵚𜵛𜵜𜵝𜵞𜵟𜵠𜵡𜵢𜵣𜵤𜵥𜵦𜵧𜵨𜵩𜵪𜵫" \
	"𜵬𜵭𜵮𜵯𜵰𜵱𜵲𜵳𜵴𜵵𜵶𜵷𜵸𜵹𜵺𜵻𜵼𜵽𜵾𜵿"
#define MOSAIC_OCTANTS_2                                                                           \
	"𜶀𜶁𜶂𜶃𜶄𜶅𜶆𜶇𜶈𜶉𜶊𜶋𜶌𜶍𜶎𜶏𜶐𜶑𜶒𜶓𜶔𜶕" \
	"𜶖𜶗𜶘𜶙𜶚𜶛𜶜𜶝𜶞𜶟𜶠𜶡𜶢𜶣𜶤𜶥𜶦𜶧𜶨𜶩𜶪𜶫" \
	"𜶬𜶭𜶮𜶯𜶰𜶱𜶲𜶳𜶴𜶵𜶶𜶷𜶸𜶹𜶺𜶻𜶼𜶽𜶾𜶿"
#define MOSAIC_OCTANTS_3                                                                           \
	"𜷀𜷁𜷂𜷃𜷄𜷅𜷆𜷇𜷈𜷉𜷊𜷋𜷌𜷍𜷎𜷏𜷐𜷑𜷒𜷓𜷔𜷕" \
	"𜷖𜷗𜷘𜷙𜷚𜷛𜷜𜷝𜷞𜷟𜷠𜷡𜷢𜷣𜷤𜷥"
#define MOSAIC_SEXT_A "🬗🬗🬗🬗🬗🬗🬗🬗"
#define MOSAIC_SEXT_B "🬤🬤🬤🬤🬤🬤🬤🬤"
#define MOSAIC_OCT "𜶉𜶉𜶉𜶉𜶉𜶉𜶉𜶉"
#define MOSAIC_DOTS "⣿⣿⣿⣿⣿⣿⣿⣿"
#define MOSAIC_WAVE_0 "⠀⡠⠔⠉⠉⠑⠤⡀⠀⠀⠀⠀⠀⠀⠀⠀"
#define MOSAIC_WAVE_1 "⠜⠀⠀⠀⠀⠀⠀⠈⢢⠀⠀⠀⠀⠀⠀⢀"
#define MOSAIC_WAVE_2 "⠀⠀⠀⠀⠀⠀⠀⠀⠀⠑⠢⣀⣀⡠⠒⠁"
#define MOSAIC_SEXT_BAND_0 "🬭🬭🬭🬭"
#define MOSAIC_SEXT_BAND_1 "🬰🬰🬰🬰"
#define MOSAIC_SEXT_BAND_2 "🬂🬂🬂🬂"
#define MOSAIC_OCT_BAND_0 "𜷋𜷋𜷋𜷋"
#define MOSAIC_OCT_BAND_1 "𜷒𜷒𜷒𜷒"
#define MOSAIC_OCT_BAND_2 "𜴂𜴂𜴂𜴂"

/*
 * Braille, sextants and octants (#76): every Braille pattern, sextant and
 * octant in codepoint order, then figures stacked across cells: checkerboards
 * of sextants and octants, a grid of eight-dot Braille, a Braille wave, and
 * bands of sextants and octants that cross the cell boundaries; then the
 * checkerboards and the grid in yellow on blue.
 */
#define MOSAIC_BYTES                                                                               \
	"braille " MOSAIC_BRAILLE_0 "\r\n        " MOSAIC_BRAILLE_1                                \
	"\r\n        " MOSAIC_BRAILLE_2 "\r\n        " MOSAIC_BRAILLE_3                            \
	"\r\nsextants" MOSAIC_SEXTANTS "\r\noctants " MOSAIC_OCTANTS_0                             \
	"\r\n        " MOSAIC_OCTANTS_1 "\r\n        " MOSAIC_OCTANTS_2                            \
	"\r\n        " MOSAIC_OCTANTS_3 "\r\n\r\n" MOSAIC_SEXT_A " " MOSAIC_OCT " " MOSAIC_DOTS    \
	" " MOSAIC_WAVE_0 " " MOSAIC_SEXT_BAND_0 " " MOSAIC_OCT_BAND_0 "\r\n" MOSAIC_SEXT_B        \
	" " MOSAIC_OCT " " MOSAIC_DOTS " " MOSAIC_WAVE_1 " " MOSAIC_SEXT_BAND_1                    \
	" " MOSAIC_OCT_BAND_1 "\r\n" MOSAIC_SEXT_A " " MOSAIC_OCT " " MOSAIC_DOTS                  \
	" " MOSAIC_WAVE_2 " " MOSAIC_SEXT_BAND_2 " " MOSAIC_OCT_BAND_2 "\r\n\r\n" ESC              \
	"[33;44m" MOSAIC_SEXT_A " " MOSAIC_OCT " " MOSAIC_DOTS "\r\n" MOSAIC_SEXT_B " " MOSAIC_OCT \
	" " MOSAIC_DOTS ESC "[0m"

static const struct check mosaic_checks[] = {
	{'p', 8, 0, 64, 4, MOSAIC_BRAILLE_0 MOSAIC_BRAILLE_1 MOSAIC_BRAILLE_2 MOSAIC_BRAILLE_3},
	{'p', 8, 4, 60, 1, MOSAIC_SEXTANTS},
	{'p', 8, 5, 64, 3, MOSAIC_OCTANTS_0 MOSAIC_OCTANTS_1 MOSAIC_OCTANTS_2},
	{'p', 8, 8, 38, 1, MOSAIC_OCTANTS_3},
	{'p', 0, 10, 8, 3, MOSAIC_SEXT_A MOSAIC_SEXT_B MOSAIC_SEXT_A},
	{'p', 9, 10, 8, 3, MOSAIC_OCT MOSAIC_OCT MOSAIC_OCT},
	{'p', 18, 10, 8, 3, MOSAIC_DOTS MOSAIC_DOTS MOSAIC_DOTS},
	{'p', 27, 10, 16, 3, MOSAIC_WAVE_0 MOSAIC_WAVE_1 MOSAIC_WAVE_2},
	{'p', 44, 10, 4, 3, MOSAIC_SEXT_BAND_0 MOSAIC_SEXT_BAND_1 MOSAIC_SEXT_BAND_2},
	{'p', 49, 10, 4, 3, MOSAIC_OCT_BAND_0 MOSAIC_OCT_BAND_1 MOSAIC_OCT_BAND_2},
	{'p', 0, 14, 8, 2, MOSAIC_SEXT_A MOSAIC_SEXT_B},
	{'p', 9, 14, 8, 2, MOSAIC_OCT MOSAIC_OCT},
	{'p', 18, 14, 8, 2, MOSAIC_DOTS MOSAIC_DOTS},
	{0},
};

#define MOSAIC_DESC                                                                                \
	"Every Braille pattern, sextant and octant in codepoint order; then checkerboards of "     \
	"sextants and of octants, a grid of eight-dot Braille, a Braille wave, and sextant and "   \
	"octant bands that cross the boundaries between rows; then the checkerboards and grid in " \
	"yellow on blue. Correct: square dots on the same 2x4 grid in every cell; blocks on exact" \
	" halves and thirds or quarters of the cell, so the checkerboards and bands run across "   \
	"cells with no seam."

#define BLOCKS_DESC                                                                                \
	"Block elements and box drawing stacked across rows and columns: full blocks, halves "     \
	"that meet across a row and side by side, eighths, quadrants, shades, and light, "         \
	"double, heavy, rounded, diagonal and dashed lines, then blocks and lines in yellow on "   \
	"blue. Correct: blocks fill their cells and lines run from cell to cell with no gap "      \
	"between rows or columns."

/*
 * Emoji (ADR-102 step 4c), each drawn from the pinned Noto Color Emoji subset
 * where it asks for emoji presentation: wide emoji beside a narrow text one, a
 * ZWJ family, a skin tone, a flag and a keycap shaped to one glyph each, a
 * heart with VS16, VS15 and neither, an emoji between letters, under
 * underline, strike, reverse and a curly SGR 58 underline, a codepoint no font
 * covers, and an emoji written at the last column, which wraps.
 */
#define EMOJI_BYTES                                                                                \
	GRIN " " THUMB " a" SMILE "b  wide, wide, narrow text\r\n" MAN ZWJ WOMAN ZWJ GIRL          \
	     " " THUMB TONE3 " " RI_J RI_P " 1" VS16 KEYCAP                                        \
	     "  family, tone, flag, keycap\r\n" HEART VS16 " " HEART VS15 " " HEART                \
	     "  VS16, VS15, none\r\n"                                                              \
	     "a" GRIN "b  between letters\r\n" ESC "[4m" GRIN ESC "[0m " ESC "[9m" GRIN ESC        \
	     "[0m " ESC "[7m" GRIN ESC "[0m " ESC "[4:3;58:5:196m" THUMB TONE3 ESC                 \
	     "[0m  under, strike, reverse, "                                                       \
	     "curl\r\n" SHAKING "  missing\r\n" ESC "[7;48H" GRIN

/* the boxes of the emoji drawn on the default background, and in reverse */
#define EMOJI_BOXES(kind)                                                                          \
	{kind, 0, 0, 2, 1},	    /* grin */                                                     \
		{kind, 3, 0, 2, 1}, /* thumb */                                                    \
		{kind, 0, 1, 2, 1}, /* family */                                                   \
		{kind, 3, 1, 2, 1}, /* skin tone */                                                \
		{kind, 6, 1, 2, 1}, /* flag */                                                     \
		{kind, 9, 1, 2, 1}, /* keycap */                                                   \
		{kind, 0, 2, 2, 1}, /* VS16 heart */                                               \
		{kind, 1, 3, 2, 1}, /* between letters */                                          \
		{kind, 6, 4, 2, 1}  /* reverse */

/* Noto Color Emoji draws the family in greys, so it is left out of 'c' */
static const struct check emoji_checks[] = {
	EMOJI_BOXES('b'),
	{'c', 0, 0, 2, 1},
	{'c', 3, 0, 2, 1},
	{'c', 3, 1, 2, 1},
	{'c', 6, 1, 2, 1},
	{'c', 9, 1, 2, 1},
	{'c', 0, 2, 2, 1},
	{'c', 1, 3, 2, 1},
	{'c', 6, 4, 2, 1},
	{'b', 0, 5, 2, 1}, /* missing */
	{0},
};

static const struct check emoji_mono_checks[] = {
	EMOJI_BOXES('b'),
	EMOJI_BOXES('m'),
	{0},
};

#define EMOJI_DESC                                                                                 \
	"Emoji: two wide emoji and a narrow text smiley; a ZWJ family, a skin tone, a flag and a " \
	"keycap; a heart with VS16, VS15 and no selector; an emoji between letters; emoji under "  \
	"underline, strike, reverse and a red curl; a codepoint no font covers; an emoji written " \
	"at the last column, which wraps to the next line."

/* ---- images (ADR-103) ---- */

/* the z of each composition band */
#define Z_BELOW_BG (-1073741825)
#define Z_BELOW_TEXT (-1)
#define Z_ABOVE_TEXT 0

enum pattern {
	PIC_GRADIENT, /* opaque: red across, green down, blue from the seed */
	PIC_HALF,     /* the gradient, its right half at half alpha */
	PIC_DISC,     /* the gradient inside the inscribed ellipse, clear outside */
	PIC_STRIPES,  /* opaque one-pixel stripes of two colours, a box filter's test */
};

static uint32_t premul(uint32_t a, uint32_t r, uint32_t g, uint32_t b)
{
	return a << 24 | (r * a + 127) / 255 << 16 | (g * a + 127) / 255 << 8 | (b * a + 127) / 255;
}

/* pixel @x, @y of a @w x @h picture of @pattern: straight alpha, 8 bits each */
static void pattern_at(enum pattern pattern, unsigned int x, unsigned int y, unsigned int w,
		       unsigned int h, unsigned int seed, uint32_t rgba[4])
{
	uint32_t r = w > 1 ? x * 255 / (w - 1) : 0, g = h > 1 ? y * 255 / (h - 1) : 0;
	uint32_t b = (seed * 97 + 64) & 0xff, a = 255;
	long dx = 2 * (long)x + 1 - (long)w, dy = 2 * (long)y + 1 - (long)h;

	if (pattern == PIC_HALF && x >= w / 2)
		a = 128;
	if (pattern == PIC_DISC &&
	    dx * dx * (long)h * h + dy * dy * (long)w * w > (long)w * w * h * h)
		a = 0;
	if (pattern == PIC_STRIPES) {
		r = (x + y) % 2 ? 255 : 0;
		g = 255 - r;
		b = 64;
	}
	rgba[0] = r;
	rgba[1] = g;
	rgba[2] = b;
	rgba[3] = a;
}

/* a @w x @h picture of @pattern in premultiplied ARGB32; free() it */
static uint32_t *picture(enum pattern pattern, unsigned int w, unsigned int h, unsigned int seed)
{
	uint32_t *px = malloc((size_t)w * h * 4), c[4];
	unsigned int x, y;

	assert(px);
	for (y = 0; y < h; y++) {
		for (x = 0; x < w; x++) {
			pattern_at(pattern, x, y, w, h, seed, c);
			px[(size_t)y * w + x] = premul(c[3], c[0], c[1], c[2]);
		}
	}
	return px;
}

/* stores a picture and returns its key: a kitty image with @id, or a sixel */
static uint64_t add_image(struct tsm_screen *con, enum pattern pattern, unsigned int w,
			  unsigned int h, unsigned int seed, uint8_t kind, uint32_t id)
{
	struct tsm_test_image img = {.width = w, .height = h, .kind = kind, .id = id};
	uint32_t *px = picture(pattern, w, h, seed);
	uint64_t key;

	img.pixels = px;
	assert(tsm_screen_test_image(con, &img, &key) == 0);
	free(px);
	return key;
}

/* places @image at @col, @row, in @cols x @rows cells (0: its own size) */
static void place(struct tsm_screen *con, uint64_t image, uint32_t id, unsigned int col,
		  unsigned int row, unsigned int cols, unsigned int rows, int32_t z)
{
	struct tsm_test_placement pl = {.image = image,
					.id = id,
					.col = col,
					.row = row,
					.cols = cols,
					.rows = rows,
					.z = z};
	uint64_t key;

	assert(tsm_screen_test_place(con, &pl, &key) == 0);
}

static void input(struct tsm_vte *vte, const char *s)
{
	tsm_vte_input(vte, s, strlen(s));
}

/*
 * A kitty graphics transmission through the vte, as a client sends one
 * (ADR-103 step 4c): a @w x @h picture of @pattern as f=32 (straight RGBA)
 * or f=24 (RGB, opaque), base64 in chunks of 4096 with m=1 on all but the
 * last, the spec's chunking; @keys go in the first chunk. It lands at the
 * cursor; with C=1 among @keys the cursor stays.
 */
static void kitty_send(struct tsm_vte *vte, const char *keys, unsigned int f, enum pattern pattern,
		       unsigned int w, unsigned int h, unsigned int seed)
{
	static const char b64[] =
		"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
	size_t bpp = f / 8, len = (size_t)w * h * bpp, n64 = (len + 2) / 3 * 4, i, o = 0;
	uint8_t *raw = malloc(len);
	char *enc = malloc(n64 + 1), head[256];
	uint32_t c[4];
	unsigned int x, y;

	assert(raw && enc && (f == 24 || f == 32));
	for (y = 0; y < h; y++) {
		for (x = 0; x < w; x++) {
			pattern_at(pattern, x, y, w, h, seed, c);
			memcpy(raw + ((size_t)y * w + x) * bpp,
			       (uint8_t[4]){c[0], c[1], c[2], c[3]}, bpp);
		}
	}
	for (i = 0; i < len; i += 3) {
		uint32_t v = raw[i] << 16 | (i + 1 < len ? raw[i + 1] << 8 : 0) |
			     (i + 2 < len ? raw[i + 2] : 0);

		enc[o++] = b64[v >> 18 & 63];
		enc[o++] = b64[v >> 12 & 63];
		enc[o++] = i + 1 < len ? b64[v >> 6 & 63] : '=';
		enc[o++] = i + 2 < len ? b64[v & 63] : '=';
	}
	for (i = 0; i < n64; i += 4096) {
		size_t chunk = n64 - i < 4096 ? n64 - i : 4096;
		bool more = i + chunk < n64;

		if (!i)
			snprintf(head, sizeof(head), ESC "_G%s,f=%u,s=%u,v=%u,m=%d;", keys, f, w, h,
				 more);
		else
			snprintf(head, sizeof(head), ESC "_Gm=%d;", more);
		input(vte, head);
		tsm_vte_input(vte, enc + i, chunk);
		input(vte, ESC "\\");
	}
	free(enc);
	free(raw);
}

/* the same half-transparent picture in each band, under and over text */
static void images_bands(struct tsm_screen *con, struct tsm_vte *vte, unsigned int frame,
			 unsigned int cw, unsigned int ch)
{
	static const int32_t z[3] = {Z_BELOW_BG, Z_BELOW_TEXT, Z_ABOVE_TEXT};

	for (unsigned int b = 0; b < 3; b++)
		place(con, add_image(con, PIC_HALF, 24 * cw, 3 * ch, b, TSM_IMAGE_KITTY, b + 1), 0,
		      14, 3 * b, 0, 0, z[b]);
}

#define BAND_TEXT(row)                                                                             \
	ESC "[" #row ";16HText " ESC "[44m    " ESC "[0m over " ESC "[42m  " ESC "[0m pic"
#define BANDS_BYTES                                                                                \
	"lowest band" BAND_TEXT(2) ESC "[4;1Hmiddle band" BAND_TEXT(5) ESC                         \
		"[7;1Htop band" BAND_TEXT(8)

/* the band below backgrounds leaves the blue cells solid */
static const struct check bands_checks[] = {
	{'f', 20, 1, 4, 1},
	{0},
};

/*
 * Sixels over text: an opaque one whose size is not whole cells, a disc clear
 * outside it, and an opaque one that text written over its middle row splits.
 */
static void images_sixel(struct tsm_screen *con, struct tsm_vte *vte, unsigned int frame,
			 unsigned int cw, unsigned int ch)
{
	place(con, add_image(con, PIC_GRADIENT, 10 * cw - 3, 3 * ch - 5, 1, TSM_IMAGE_SIXEL, 0), 0,
	      2, 1, 0, 0, 0);
	place(con, add_image(con, PIC_DISC, 10 * cw, 3 * ch, 2, TSM_IMAGE_SIXEL, 0), 0, 16, 1, 0, 0,
	      0);
	place(con, add_image(con, PIC_GRADIENT, 10 * cw, 3 * ch, 3, TSM_IMAGE_SIXEL, 0), 0, 30, 1,
	      0, 0, 0);
	input(vte, ESC "[3;33HTEXT");
}

/*
 * Sixel strings through the vte, as a program sends them (ADR-103 step 3d):
 * four 10-pixel bars, opaque, then TEXT written across them; and orange
 * stripes 5 pixels wide with transparent gaps (P2=1) over a row of text.
 */
#define SIX_BAND "#1!10~#2!10~#3!10~#4!10~"
#define SIX_STRIPES "!5~!5?!5~!5?!5~!5?"
#define SIXEL_VTE_BYTES                                                                            \
	"sixel strings through the vte\r\n"                                                        \
	"\r\n"                                                                                     \
	"                    text text text text\r\n"                                              \
	"                    text text text text\r\n" ESC "[3;2H" ESC "Pq\"1;1;40;24"              \
	"#1;2;100;0;0#2;2;0;100;0#3;2;0;0;100#4;2;100;100;100" SIX_BAND "-" SIX_BAND "-" SIX_BAND  \
	"-" SIX_BAND ESC "\\" ESC "[4;4HTEXT" ESC "[3;21H" ESC                                     \
	"P0;1q\"1;1;30;24#5;2;100;50;0#5" SIX_STRIPES "-#5" SIX_STRIPES "-#5" SIX_STRIPES          \
	"-#5" SIX_STRIPES ESC "\\"

#define TEXT_ROW "sixels over text, text over sixels, sixels over\r\n"
#define SIXEL_BYTES TEXT_ROW TEXT_ROW TEXT_ROW TEXT_ROW TEXT_ROW

/*
 * Scaled pictures, all in the top band: one at its own size; the same fitted
 * into 10 x 5 cells; a 480 x 160 picture of one-pixel stripes shrunk into
 * 12 x 2 cells; and one made in 5 x 8 pixel cells, drawn in the font's.
 */
static void images_scale(struct tsm_screen *con, struct tsm_vte *vte, unsigned int frame,
			 unsigned int cw, unsigned int ch)
{
	uint64_t small = add_image(con, PIC_GRADIENT, 40, 20, 1, TSM_IMAGE_KITTY, 1);

	place(con, small, 0, 1, 1, 0, 0, 0);
	place(con, small, 0, 7, 1, 10, 5, 0);
	place(con, add_image(con, PIC_STRIPES, 480, 160, 0, TSM_IMAGE_KITTY, 2), 0, 19, 1, 12, 2,
	      0);
	assert(tsm_screen_set_cell_size(con, 5, 8) == 0);
	place(con, add_image(con, PIC_GRADIENT, 20, 16, 2, TSM_IMAGE_KITTY, 3), 0, 33, 1, 0, 0, 0);
	assert(tsm_screen_set_cell_size(con, cw, ch) == 0);
}

#define SCALE_BYTES ESC "[8;2H1:1" ESC "[8;8Hfit 10x5" ESC "[8;20Hshrunk 12x2" ESC "[8;34H5x8 cells"

/*
 * A region scroll: a sixel inside the region loses the rows pushed out of its
 * top, a kitty picture wholly inside moves and is clipped at the margin, and
 * one that straddles the margin stays.
 */
static void images_region(struct tsm_screen *con, struct tsm_vte *vte, unsigned int frame,
			  unsigned int cw, unsigned int ch)
{
	place(con, add_image(con, PIC_GRADIENT, 8 * cw, 4 * ch, 1, TSM_IMAGE_SIXEL, 0), 0, 2, 2, 0,
	      0, 0);
	place(con, add_image(con, PIC_GRADIENT, 6 * cw, 2 * ch, 2, TSM_IMAGE_KITTY, 1), 0, 16, 3, 0,
	      0, 0);
	place(con, add_image(con, PIC_GRADIENT, 6 * cw, 4 * ch, 3, TSM_IMAGE_KITTY, 2), 0, 28, 1, 0,
	      0, 0);
	input(vte, ESC "[3;8r" ESC "[8;1H\n\n" ESC "[r");
}

#define REGION_BYTES                                                                               \
	"row 0\r\nrow 1\r\nrow 2\r\nrow 3\r\nrow 4\r\nrow 5\r\nrow 6\r\nrow 7\r\nrow 8\r\nrow 9"

/* a picture at its own size and fitted to 6 x 3 cells, on a display turned right */
static void images_rotated(struct tsm_screen *con, struct tsm_vte *vte, unsigned int frame,
			   unsigned int cw, unsigned int ch)
{
	uint64_t pic = add_image(con, PIC_HALF, 4 * cw, 2 * ch, 1, TSM_IMAGE_KITTY, 1);

	place(con, pic, 0, 2, 1, 0, 0, 0);
	place(con, pic, 0, 10, 1, 6, 3, Z_BELOW_TEXT);
}

/*
 * Frame 0: two kitty pictures, a sixel and text. Frame 2: the first kitty
 * placement moved by replacing it, the second removed by sending its image
 * again, which deletes its placements and leaves the text under it as it was,
 * the sixel erased with its rows' text, and a new picture.
 */
static void images_moves(struct tsm_screen *con, struct tsm_vte *vte, unsigned int frame,
			 unsigned int cw, unsigned int ch)
{
	static uint64_t kitty;

	if (frame == 0) {
		kitty = add_image(con, PIC_GRADIENT, 8 * cw, 3 * ch, 1, TSM_IMAGE_KITTY, 1);
		place(con, kitty, 1, 2, 1, 0, 0, 0);
		place(con, add_image(con, PIC_DISC, 8 * cw, 2 * ch, 2, TSM_IMAGE_SIXEL, 0), 0, 20,
		      1, 0, 0, 0);
		place(con, add_image(con, PIC_GRADIENT, 4 * cw, 2 * ch, 4, TSM_IMAGE_KITTY, 3), 0,
		      40, 3, 0, 0, 0);
	} else if (frame == 2) {
		place(con, kitty, 1, 6, 5, 0, 0, 0);
		add_image(con, PIC_GRADIENT, 4 * cw, 2 * ch, 5, TSM_IMAGE_KITTY, 3);
		input(vte, ESC "[2;21H" ESC "[K" ESC "[3;21H" ESC "[K");
		place(con, add_image(con, PIC_GRADIENT, 6 * cw, 2 * ch, 3, TSM_IMAGE_KITTY, 2), 0,
		      30, 6, 0, 0, Z_BELOW_TEXT);
	}
}

#define MOVES_BYTES TEXT_ROW TEXT_ROW TEXT_ROW TEXT_ROW TEXT_ROW TEXT_ROW TEXT_ROW TEXT_ROW

/*
 * ADR-103's A6: a picture over the whole screen, replaced on each of frames 0
 * to 2, and a banner of coloured text over its bottom rows that scrolls in its
 * region with it; below backgrounds or below text. Frame 3 repeats frame 2, so
 * both buffers end with the same screen.
 */
static void banner(struct tsm_screen *con, struct tsm_vte *vte, unsigned int frame, unsigned int cw,
		   unsigned int ch, int32_t z)
{
	char line[128];

	if (frame == 3)
		return;

	place(con, add_image(con, PIC_GRADIENT, COLS * cw, ROWS * ch, frame, TSM_IMAGE_KITTY, 1), 0,
	      0, 0, 0, 0, z);
	snprintf(line, sizeof(line),
		 "\r\n" ESC "[44;97m banner %u " ESC "[0m text " ESC "[42m over " ESC "[0m video",
		 frame);
	input(vte, line);
}

static void images_banner_bg(struct tsm_screen *con, struct tsm_vte *vte, unsigned int frame,
			     unsigned int cw, unsigned int ch)
{
	banner(con, vte, frame, cw, ch, Z_BELOW_BG);
}

static void images_banner_text(struct tsm_screen *con, struct tsm_vte *vte, unsigned int frame,
			       unsigned int cw, unsigned int ch)
{
	banner(con, vte, frame, cw, ch, Z_BELOW_TEXT);
}

#define BANNER_BYTES ESC "[9;10r" ESC "[10;1H"

/* ADR-103's A3: pictures shown for two frames, then ED 2 */
static void images_ed2(struct tsm_screen *con, struct tsm_vte *vte, unsigned int frame,
		       unsigned int cw, unsigned int ch)
{
	if (frame)
		return;
	place(con, add_image(con, PIC_GRADIENT, 12 * cw, 4 * ch, 1, TSM_IMAGE_SIXEL, 0), 0, 4, 1, 0,
	      0, 0);
	place(con, add_image(con, PIC_DISC, 8 * cw, 4 * ch, 2, TSM_IMAGE_KITTY, 1), 0, 24, 2, 0, 0,
	      Z_BELOW_TEXT);
}

/*
 * Step 4c: the same three pictures as img-bands, transmitted and placed by
 * kitty commands through the vte, at the cursor with C=1.
 */
static void kitty_bands(struct tsm_screen *con, struct tsm_vte *vte, unsigned int frame,
			unsigned int cw, unsigned int ch)
{
	static const char *const keys[3] = {"a=T,i=1,C=1,z=-1073741825", "a=T,i=2,C=1,z=-1",
					    "a=T,i=3,C=1,z=0"};
	char cup[32];

	for (unsigned int b = 0; b < 3; b++) {
		snprintf(cup, sizeof(cup), ESC "7" ESC "[%u;15H", 3 * b + 1);
		input(vte, cup);
		kitty_send(vte, keys[b], 32, PIC_HALF, 24 * cw, 3 * ch, b);
		input(vte, ESC "8");
	}
}

/*
 * Text over kitty pictures: one placed with C=0, which leaves the cursor
 * after it on its last row, where the text that follows goes; text written
 * over it afterwards; and a disc in the top band whose row is erased with
 * EL, which only ED 2 takes from a kitty picture. The left picture is f=24.
 */
static void kitty_text(struct tsm_screen *con, struct tsm_vte *vte, unsigned int frame,
		       unsigned int cw, unsigned int ch)
{
	input(vte, ESC "[3;2H");
	kitty_send(vte, "a=T,i=1,z=-1", 24, PIC_GRADIENT, 8 * cw, 3 * ch, 1);
	input(vte, "<after");
	input(vte, ESC "[4;3H" ESC "[97;41mTEXT" ESC "[0m on it");
	input(vte, ESC "[3;26H");
	kitty_send(vte, "a=T,i=2,C=1,z=0", 32, PIC_DISC, 10 * cw, 3 * ch, 2);
	input(vte, ESC "[4;20H" ESC "[K" ESC "[8;1H");
}

#define KITTY_TEXT_BYTES                                                                           \
	"kitty pictures under and over text" ESC "[3;26Hfirst row, around it" ESC                  \
	"[4;26Hmiddle row, erased" ESC "[5;26Hlast row, around it"

/*
 * ADR-103's A6 through the vte: each frame transmits a new full-screen
 * picture as image 1, which replaces the last and its placement, at the top
 * left, then a banner line scrolls in the bottom two rows.
 */
static void kitty_banner(struct tsm_vte *vte, unsigned int frame, unsigned int cw, unsigned int ch,
			 const char *keys)
{
	char line[128];

	if (frame == 3)
		return;
	input(vte, ESC "7" ESC "[1;1H");
	kitty_send(vte, keys, frame % 2 ? 24 : 32, PIC_GRADIENT, COLS * cw, ROWS * ch, frame);
	input(vte, ESC "8");
	snprintf(line, sizeof(line),
		 "\r\n" ESC "[44;97m banner %u " ESC "[0m text " ESC "[42m over " ESC "[0m video",
		 frame);
	input(vte, line);
}

static void kitty_banner_bg(struct tsm_screen *con, struct tsm_vte *vte, unsigned int frame,
			    unsigned int cw, unsigned int ch)
{
	kitty_banner(vte, frame, cw, ch, "a=T,i=1,q=2,C=1,z=-1073741825");
}

static void kitty_banner_text(struct tsm_screen *con, struct tsm_vte *vte, unsigned int frame,
			      unsigned int cw, unsigned int ch)
{
	kitty_banner(vte, frame, cw, ch, "a=T,i=1,q=2,C=1,z=-1");
}

static const struct screen screens[] = {
	{"ascii",
	 "Printable ASCII in the regular face: a pangram in both cases, digits and punctuation, "
	 "runs of narrow and wide letters, and a line of code. Correct: every glyph whole and "
	 "inside its own cell, with columns aligned down the screen.",
	 "The quick brown fox jumps over the lazy dog.\r\n"
	 "THE QUICK BROWN FOX JUMPS OVER THE LAZY DOG.\r\n"
	 "0123456789 !\"#$%&'()*+,-./:;<=>?@[\\]^_`{|}~\r\n"
	 "iiiiiiii llllllll WWWWWWWW mmmmmmmm\r\n"
	 "func main() { return 0; }  // 80x24 \r\n"},

	{"bold",
	 "Bold beside regular, a bold pangram over two lines, and bold in red, green and blue. "
	 "Correct: the bold text drawn in the bold face at the same cell width, the coloured words"
	 " in their colours.",
	 "regular " ESC "[1mbold" ESC "[0m regular\r\n" ESC "[1mThe quick brown fox jumps over\r\n"
	 "the lazy dog 0123456789 !@#$%^&*()" ESC "[0m\r\n" ESC "[1;31mbold red" ESC "[0m " ESC
	 "[1;32mbold green" ESC "[0m " ESC "[1;34mbold blue" ESC "[0m\r\n"},

	{"palette",
	 "The 16 ANSI background colours, the 16 foreground colours on letter pairs, a ramp "
	 "through the 256-colour palette, and a 24-bit gradient from red to green. Correct: each "
	 "block filled edge to edge in its own colour, with no gaps between cells.",
	 /* 16 ANSI backgrounds, then foregrounds, then a 256-colour ramp and a
	  * truecolour gradient */
	 ESC "[40m  " ESC "[41m  " ESC "[42m  " ESC "[43m  " ESC "[44m  " ESC "[45m  " ESC
	     "[46m  " ESC "[47m  " ESC "[100m  " ESC "[101m  " ESC "[102m  " ESC "[103m  " ESC
	     "[104m  " ESC "[105m  " ESC "[106m  " ESC "[107m  " ESC "[0m\r\n" ESC "[30mAB" ESC
	     "[31mAB" ESC "[32mAB" ESC "[33mAB" ESC "[34mAB" ESC "[35mAB" ESC "[36mAB" ESC
	     "[37mAB" ESC "[90mAB" ESC "[91mAB" ESC "[92mAB" ESC "[93mAB" ESC "[94mAB" ESC
	     "[95mAB" ESC "[96mAB" ESC "[97mAB" ESC "[0m\r\n" ESC "[48;5;16m  " ESC
	     "[48;5;52m  " ESC "[48;5;88m  " ESC "[48;5;124m  " ESC "[48;5;160m  " ESC
	     "[48;5;196m  " ESC "[48;5;202m  " ESC "[48;5;214m  " ESC "[48;5;226m  " ESC
	     "[48;5;154m  " ESC "[48;5;46m  " ESC "[48;5;48m  " ESC "[48;5;51m  " ESC
	     "[48;5;27m  " ESC "[48;5;21m  " ESC "[48;5;129m  " ESC "[48;5;232m  " ESC
	     "[48;5;244m  " ESC "[48;5;255m  " ESC "[0m\r\n" ESC "[48;2;255;0;0m  " ESC
	     "[48;2;223;32;0m  " ESC "[48;2;191;64;0m  " ESC "[48;2;159;96;0m  " ESC
	     "[48;2;127;128;0m  " ESC "[48;2;95;160;0m  " ESC "[48;2;63;192;0m  " ESC
	     "[48;2;31;224;0m  " ESC "[48;2;0;255;0m  " ESC "[0m\r\n"},

	{"inverse",
	 "Reverse video (SGR 7): one word, a whole line, red on green swapped, and bold with "
	 "reverse. Correct: foreground and background exchanged, so the swapped word is green on "
	 "red.",
	 "normal " ESC "[7minverse" ESC "[0m normal\r\n" ESC "[7mWhole line inverted" ESC
	 "[0m\r\n" ESC "[31;42mred on green " ESC "[7mswapped" ESC "[0m\r\n" ESC
	 "[1;7mbold inverse" ESC "[0m\r\n"},

	{"cjk",
	 "Double-width Japanese, Chinese and Korean, full-width Latin, narrow letters between wide"
	 " characters, and wide text in bold and in reverse. Correct: each wide character takes "
	 "two cells and the narrow letters between them keep their own columns.",
	 "日本語の端末表示 テスト\r\n"
	 "中文字符 漢字 ひらがな カタカナ\r\n"
	 "한글 테스트\r\n"
	 "ＡＢＣＤ０１２３ abcd 0123\r\n"
	 "a日b本c語d\r\n" ESC "[1m日本語 bold" ESC "[0m " ESC "[7m日本語 inverse" ESC "[0m\r\n"},

	{"box",
	 "Box drawing in light and double lines, shades and block elements, eighth blocks, and "
	 "rounded, heavy and diagonal pieces. Correct: lines join from cell to cell with no gaps, "
	 "and blocks fill their cells.",
	 "┌──────┬──────┐ ╔══════╦══════╗\r\n"
	 "│ cell │ cell │ ║ cell ║ cell ║\r\n"
	 "├──────┼──────┤ ╠══════╬══════╣\r\n"
	 "│      │      │ ║      ║      ║\r\n"
	 "└──────┴──────┘ ╚══════╩══════╝\r\n"
	 "░░▒▒▓▓██ ▀▄▌▐ ▁▂▃▄▅▆▇█ ▏▎▍▌▋▊▉█\r\n"
	 "╭─╮ ╰─╯ ┏━┓ ┗━┛ ┃ ╱╲╳\r\n"},

	/* ADR-102: each underline style in the foreground and in an SGR 58
	 * colour, over descenders */
	{"underline",
	 "Each underline style (single, double, curly, dotted, dashed) in the text colour and in "
	 "an SGR 58 colour, over descenders; a red curl ended by SGR 59; underlines on a "
	 "background, in reverse and in bold. Correct: the five styles distinct, the coloured ones"
	 " in their SGR 58 colour, and the line after SGR 59 in the text colour.",
	 ESC "[4mSingle gjpqy" ESC "[0m  " ESC "[4;58:5:196mSingle gjpqy" ESC "[0m\r\n" ESC
	     "[4:2mDouble gjpqy" ESC "[0m  " ESC "[4:2;58:2::0:200:255mDouble gjpqy" ESC
	     "[0m\r\n" ESC "[4:3mCurly  gjpqy" ESC "[0m  " ESC "[4:3;58:5:46mCurly  gjpqy" ESC
	     "[0m\r\n" ESC "[4:4mDotted gjpqy" ESC "[0m  " ESC
	     "[4:4;58:2::255:128:0mDotted gjpqy" ESC "[0m\r\n" ESC "[4:5mDashed gjpqy" ESC
	     "[0m  " ESC "[4:5;58:5:201mDashed gjpqy" ESC "[0m\r\n" ESC
	     "[31;4:3;58:5:226mred, yellow curl" ESC "[59m then fg" ESC "[0m\r\n" ESC
	     "[44;4mon blue" ESC "[0m " ESC "[7;4minverse" ESC "[0m " ESC "[1;4:2mbold double" ESC
	     "[0m\r\n"},

	{"strike",
	 "Strike-through (SGR 9) alone, in colour, on a background, with straight and curly "
	 "underlines, in bold, in reverse, and over spaces. Correct: a line through the middle of "
	 "every struck cell, the spaces included.",
	 ESC "[9mstruck through gjpqy" ESC "[0m plain\r\n" ESC "[9;31mred strike" ESC "[0m " ESC
	     "[9;42mstrike on green" ESC "[0m\r\n" ESC "[9;4mstrike and underline" ESC "[0m " ESC
	     "[9;4:3;58:5:51mcurly too" ESC "[0m\r\n" ESC "[1;9mbold struck" ESC "[0m " ESC
	     "[7;9minverse struck" ESC "[0m\r\n" ESC "[9m      " ESC "[0m struck spaces\r\n"},

	/* underline-color=196, strike-color=#00c0ff */
	{"deco-config",
	 "Underline and strike with the configured colours (underline-color=196, "
	 "strike-color=#00c0ff), beside SGR 58 colours and coloured text. Correct: underlines and "
	 "strikes without an SGR 58 colour take the configured colours; an SGR 58 colour still "
	 "applies.",
	 DECO_CONFIG_BYTES, NULL, &conf_deco},
	/* the same, with force-decoration-colors */
	{"deco-force",
	 "The deco-config screen with force-decoration-colors set. Correct: every underline and "
	 "strike in the configured colour, the SGR 58 ones included.",
	 DECO_CONFIG_BYTES, NULL, &conf_deco_force},

	/* clusters shaped by HarfBuzz: decomposed against precomposed */
	{"combining",
	 "Accented letters precomposed and as a base letter with combining marks, in words, "
	 "underlined, struck and in bold, shaped by HarfBuzz. Correct: each mark sits on its base "
	 "letter in one cell, matching the precomposed form.",
	 "precomposed: " E_ACUTE " " A_TILDE " " N_TILDE "\r\n"
	 "decomposed:  e" COMB_ACUTE " a" COMB_TILDE COMB_GRAVE " n" COMB_DIAER "\r\n"
	 "in words: cafe" COMB_ACUTE " na" COMB_TILDE COMB_GRAVE "o n" COMB_DIAER "ice\r\n" ESC
	 "[4mu" COMB_DIAER "nderlined e" COMB_ACUTE ESC "[0m " ESC
	 "[9mstruck a" COMB_TILDE COMB_GRAVE ESC "[0m\r\n"
	 "bold: " ESC "[1me" COMB_ACUTE " a" COMB_TILDE COMB_GRAVE " n" COMB_DIAER ESC "[0m\r\n"},

	/* a heart with VS16 takes two cells and draws as the colour emoji */
	{"vs16",
	 "A heart with VS16 (two cells), with no selector and with VS15 (one cell each), and the "
	 "VS16 heart underlined. Correct: the b after the VS16 heart one column further right; "
	 "that heart is the red colour emoji fitted to its two cells, the others the text heart.",
	 "a" HEART VS16 "b  VS16, wide\r\n"
	 "a" HEART "b  no selector, narrow\r\n"
	 "a" HEART VS15 "b  VS15, narrow\r\n" ESC "[4ma" HEART VS16 "b" ESC "[0m  underlined\r\n"},

	/* decorated wide cells take each sprite twice; the last one is cut at
	 * the margin */
	{"wide-deco",
	 "Wide characters under each underline style and under strike, a decorated run mixing "
	 "narrow and wide, and a decorated wide character placed in the last column. Correct: "
	 "every decoration spans both cells of its wide character; the one in the last column is "
	 "cut at the edge.",
	 ESC "[4m日本語" ESC "[0m " ESC "[4:3m漢字" ESC "[0m " ESC "[4:4;58:5:196mカタカナ" ESC
	     "[0m\r\n" ESC "[4:2m한글" ESC "[0m " ESC "[4:5m中文" ESC "[0m " ESC "[9m日本語" ESC
	     "[0m\r\n" ESC "[4ma日b本c" ESC "[0m\r\n" ESC "[5;48H" ESC "[4;9m語" ESC "[0m"},

	/* #46: drawn to the cell, not the font's glyph box */
	{"blocks", BLOCKS_DESC " The cell is the font's own line height.", BLOCKS_BYTES, NULL, NULL,
	 0, 0, blocks_checks},
	{"blocks-spaced",
	 BLOCKS_DESC " The cell is 6 pixel rows taller than the font's line height, as with a font"
		     " whose line height has spacing beyond its glyphs.",
	 BLOCKS_BYTES, NULL, NULL, 22, 6, blocks_checks},

	/* #76: Braille dots and mosaic blocks on the cell's grid */
	{.name = "mosaic",
	 .desc = MOSAIC_DESC " The cell is the font's own line height.",
	 .bytes = MOSAIC_BYTES,
	 .checks = mosaic_checks,
	 .cols = 72,
	 .rows = 16},
	{.name = "mosaic-spaced",
	 .desc = MOSAIC_DESC
	 " The cell is 6 pixel rows taller than the font's line height, as with "
	 "a font whose line height has spacing beyond its glyphs.",
	 .bytes = MOSAIC_BYTES,
	 .px = 22,
	 .spacing = 6,
	 .checks = mosaic_checks,
	 .cols = 72,
	 .rows = 16},

	/* ADR-102 step 4c: colour emoji fitted to their cells */
	{"emoji",
	 EMOJI_DESC
	 " Correct: each emoji in colour, whole and centred in its two cells with a "
	 "pixel clear at every edge; the family, tone, flag and keycap one picture each; the"
	 " VS15 and plain hearts and the smiley narrow text; the missing one the text font's"
	 " missing-glyph box, fitted the same way.",
	 EMOJI_BYTES, NULL, NULL, 0, 0, emoji_checks},
	{"emoji-spaced",
	 EMOJI_DESC
	 " The font is larger and the cell is 6 pixel rows taller than its line height. "
	 "Correct: as the emoji screen, each emoji fitted to the taller cell and centred in "
	 "it.",
	 EMOJI_BYTES, NULL, NULL, 22, 6, emoji_checks},
	{"emoji-mono",
	 EMOJI_DESC
	 " With no-emoji-color. Correct: every emoji drawn in the grey text colour, as "
	 "its shape, at the size and place the emoji screen draws it in colour; the hearts "
	 "and the smiley from the text font.",
	 EMOJI_BYTES, NULL, NULL, 0, 0, emoji_mono_checks, true},

	/* frame N changes cells that frame N+1 must draw again into the other
	 * buffer */
	{"redraw",
	 "Two frames into the double-buffered display: the second rewrites line 2 and adds a "
	 "struck line 4, and both buffers must then hold it. Correct: line 2 reads 'line 2 CHANGED"
	 " curly' with nothing left of the first frame's text.",
	 "unchanged line\r\n"
	 "line two, before\r\n"
	 "line three stays\r\n",
	 ESC "[2;1Hline 2 CHANGED " ESC "[4:3mcurly" ESC "[0m" ESC "[4;1H" ESC "[9mnew struck" ESC
	     "[0m"},

	/* ADR-103 step 3b: placements made through libtsm's test calls */
	{.name = "img-bands",
	 .desc = "The same picture in each composition band, its left half opaque and its right "
		 "half "
		 "at half alpha, behind a row of text with blue and green backgrounds. Correct: in "
		 "the lowest band the picture shows only through cells on the default background, "
		 "so "
		 "the blue and green cells stay solid, and the text draws over it; in the middle "
		 "band "
		 "the picture covers the backgrounds and the text draws over it; in the top band "
		 "the "
		 "left half hides the text and the right half shows it dimmed through.",
	 .bytes = BANDS_BYTES,
	 .checks = bands_checks,
	 .images = images_bands},
	{.name = "img-sixel",
	 .desc = "Three sixels over rows of text. Correct: the left picture hides the text under "
		 "it, "
		 "its right and bottom edges ending inside their cells with the text there showing "
		 "past them; the middle one is a disc with the text visible around it; the right "
		 "one "
		 "has a hole in its middle row where TEXT was written over it, drawn as plain "
		 "text.",
	 .bytes = SIXEL_BYTES,
	 .images = images_sixel},
	{.name = "sixel-vte",
	 .desc = "Two sixel strings parsed by the vte, as a program sends them. Correct: at the "
		 "left, bars of red, green, blue and white side by side, the word TEXT across "
		 "their second row drawn as plain text with no picture behind it; at the right, "
		 "orange vertical stripes with gaps through which the text under them shows.",
	 .bytes = SIXEL_VTE_BYTES},
	{.name = "img-scale",
	 .desc = "Scaled pictures in the top band: a 40x20 gradient at its own size; the same "
		 "fitted "
		 "and centred in 10x5 cells, letterboxed; 480x160 one-pixel stripes shrunk into "
		 "12x2 "
		 "cells, which a box filter turns into an even mix of the two colours; and a 20x16 "
		 "picture made in 5x8-pixel cells, drawn over the 4x2 font cells it was given. "
		 "Correct: smooth gradients with no blocky steps, the stripes an even olive with "
		 "no "
		 "moire, each picture inside its labelled place.",
	 .bytes = SCALE_BYTES,
	 .images = images_scale},
	{.name = "img-region",
	 .desc = "A scroll of two lines in a region from row 2 to row 7 with three pictures on "
		 "screen. Correct: the sixel, inside the region from its top, has lost its top two "
		 "rows and its lower half now starts at row 2; the small kitty picture, wholly "
		 "inside, moved up and lost the row pushed past the margin; the tall kitty picture "
		 "at the right, straddling the margin, stays where it was while the rows under it "
		 "move.",
	 .bytes = REGION_BYTES,
	 .images = images_region},
	{.name = "img-rotated",
	 .desc = "A display turned right: text, a picture at its own size and the same picture "
		 "fitted to 6x3 cells under the text. Correct: the pictures turn with the text, "
		 "their opaque halves toward the top of the image, each in its own cells.",
	 .bytes = "rotated: pictures turn with the text",
	 .images = images_rotated,
	 .cols = 24,
	 .rows = 6,
	 .orientation = OR_RIGHT},
	{.name = "img-moves",
	 .desc = "Four frames into the double-buffered display: two kitty pictures and a sixel, "
		 "then the first kitty placement moved by replacing it, the second removed by "
		 "sending its image again with the text under it unchanged, the sixel erased with "
		 "the text of its rows, and a new picture added under the text. Both buffers must "
		 "then hold what a fresh draw of the last screen draws. Correct: no trace of the "
		 "picture's old place, of the removed picture at the right or of the sixel.",
	 .bytes = MOVES_BYTES,
	 .next = "",
	 .images = images_moves},
	{.name = "img-banner-bg",
	 .desc = "ADR-103's A6 below backgrounds: a full-screen picture replaced on three frames "
		 "running, under a banner of text that scrolls in the bottom two rows with it. "
		 "Correct: the last frame's picture, the banner's blue and green backgrounds solid "
		 "over it, and both buffers equal to a fresh draw.",
	 .bytes = BANNER_BYTES,
	 .next = "",
	 .images = images_banner_bg},
	{.name = "img-banner-text",
	 .desc = "ADR-103's A6 below text: the same, with the picture over the cell backgrounds. "
		 "Correct: the picture under every cell, only the banner's glyphs over it, and "
		 "both "
		 "buffers equal to a fresh draw.",
	 .bytes = BANNER_BYTES,
	 .next = "",
	 .images = images_banner_text},
	{.name = "img-ed2",
	 .desc = "ADR-103's A3: a sixel and a kitty picture shown for two frames, then ED 2 and a "
		 "word. Correct: only the word, with no pixel of either picture left in either "
		 "buffer.",
	 .bytes = TEXT_ROW TEXT_ROW TEXT_ROW TEXT_ROW TEXT_ROW TEXT_ROW,
	 .next = ESC "[2J" ESC "[Hcleared",
	 .images = images_ed2},

	/* ADR-103 step 4c: kitty commands parsed by the vte, as a program sends them */
	{.name = "kitty-bands",
	 .desc = "img-bands through the kitty graphics protocol: each picture transmitted in "
		 "4096-byte chunks and placed at the cursor by a kitty command with C=1, at "
		 "z=-1073741825, -1 and 0. Correct: as img-bands; in the lowest band the blue and "
		 "green cells stay solid with the picture showing round them, in the middle band "
		 "the picture covers the backgrounds, in the top band its left half hides the "
		 "text and its right half shows it dimmed.",
	 .bytes = BANDS_BYTES,
	 .checks = bands_checks,
	 .images = kitty_bands},
	{.name = "kitty-text",
	 .desc = "Text and kitty pictures: at the left an opaque f=24 gradient in the middle "
		 "band, placed with C=0, so the text that follows, <after, starts on its last row "
		 "just past its right edge; TEXT, white on red, and 'on it' written over its "
		 "middle row afterwards, the red under the picture, which the middle band draws "
		 "over cell backgrounds. At the right a disc in the top band over three rows of "
		 "text, the middle row then erased by EL from column 20. Correct: the gradient "
		 "whole under the white text written on it, no red showing; the disc whole, the "
		 "first and last rows' "
		 "words showing only outside it, and nothing of the middle row's.",
	 .bytes = KITTY_TEXT_BYTES,
	 .images = kitty_text},
	{.name = "kitty-banner-bg",
	 .desc = "ADR-103's A6 through the kitty graphics protocol, below backgrounds: a "
		 "full-screen picture transmitted again as the same image on three frames running, "
		 "f=32 and f=24 in turn, each replacing the last, under a banner of text that "
		 "scrolls in the bottom two rows. Correct: the last frame's picture alone, the "
		 "banner's blue and green backgrounds solid over it, and both buffers equal to a "
		 "fresh draw.",
	 .bytes = BANNER_BYTES,
	 .next = "",
	 .images = kitty_banner_bg},
	{.name = "kitty-banner-text",
	 .desc = "ADR-103's A6 through the kitty graphics protocol, below text: the same, with "
		 "the picture over the cell backgrounds. Correct: the picture under every cell, "
		 "only the banner's glyphs over it, and both buffers equal to a fresh draw.",
	 .bytes = BANNER_BYTES,
	 .next = "",
	 .images = kitty_banner_text},

	/* ADR-103's render timer display: the first placement bbulk draws */
	{.name = "osd",
	 .desc = "The render timer's on-screen display over text, then replaced by a shorter "
		 "line two frames later. Correct: the figures in light grey on a dark grey box at "
		 "the right end of the top row, over the text there; the longer first line leaves "
		 "nothing behind, the text it covered drawn again; both buffers equal a fresh "
		 "draw.",
	 .bytes = TEXT_ROW TEXT_ROW TEXT_ROW,
	 .next = "",
	 .osd = "59.9 fps 12.345 ms 100.0%",
	 .osd_next = "7.5 fps 1.234 ms 2.5%"},
};

static void vte_write(struct tsm_vte *vte, const char *u8, size_t len, void *data) {}

static int write_ppm(const char *path)
{
	FILE *f = fopen(path, "wb");
	unsigned int x, y;

	if (!f)
		return -errno;
	fprintf(f, "P6\n%u %u\n255\n", fb_w, fb_h);
	for (y = 0; y < fb_h; y++) {
		for (x = 0; x < fb_w; x++) {
			uint32_t p = fb[y * fb_w + x];
			uint8_t rgb[3] = {p >> 16, p >> 8, p};

			fwrite(rgb, 1, 3, f);
		}
	}
	return fclose(f) ? -errno : 0;
}

static int write_text(const char *path, const char *text)
{
	FILE *f = fopen(path, "w");

	if (!f)
		return -errno;
	fprintf(f, "%s\n", text);
	return fclose(f) ? -errno : 0;
}

static bool is_grey(uint32_t p)
{
	return ((p >> 16) & 0xff) == ((p >> 8) & 0xff) && ((p >> 8) & 0xff) == (p & 0xff);
}

/* an emoji check ('b', 'c' or 'm') on the box x0,y0 to x1,y1; NULL if it holds */
static const char *check_box(char kind, unsigned int x0, unsigned int y0, unsigned int x1,
			     unsigned int y1)
{
	uint32_t bg = fb[y0 * fb_w + x0];
	bool ink = false, colour = false;

	for (unsigned int y = y0; y < y1; y++) {
		for (unsigned int x = x0; x < x1; x++) {
			uint32_t p = fb[y * fb_w + x];
			bool ring = x == x0 || x == x1 - 1 || y == y0 || y == y1 - 1;

			if (kind == 'b' && ring && p != bg)
				return "ink on the edge of the box (cut, or not fitted)";
			ink |= p != bg;
			colour |= !is_grey(p);
		}
	}
	if (kind == 'b' && !ink)
		return "nothing drawn in the box";
	if (kind == 'c' && !colour)
		return "no colour in the box";
	if (kind == 'm' && colour)
		return "colour in the box";
	return NULL;
}

/* the codepoint at *@s, which moves past it */
static uint32_t utf8_next(const char **s)
{
	const unsigned char *p = (const unsigned char *)*s;
	unsigned int n = *p < 0x80 ? 1 : *p < 0xe0 ? 2 : *p < 0xf0 ? 3 : 4;
	uint32_t cp = n == 1 ? *p : *p & (0x3f >> (n - 1));

	for (unsigned int i = 1; i < n; i++)
		cp = cp << 6 | (p[i] & 0x3f);
	*s += n;
	return cp;
}

/* a 'p' check on the cells at @col, @row; NULL if it holds */
static const char *check_mosaic(const struct check *c, unsigned int cw, unsigned int ch)
{
	static const char *const bad[2] = {
		"a pixel the sprite leaves clear is not the background",
		"a pixel the sprite covers is not the ink colour",
	};
	uint8_t *want = malloc((size_t)cw * ch);
	const char *s = c->cells, *why = NULL;
	uint32_t on = 0, off = 0;
	bool seen_on = false, seen_off = false;

	assert(want);
	if ((c->col + c->cols) * cw > fb_w || (c->row + c->rows) * ch > fb_h)
		why = "the check's cells reach past the screen";
	for (unsigned int r = 0; r < c->rows && !why; r++) {
		for (unsigned int k = 0; k < c->cols && !why; k++) {
			uint32_t cp = *s ? utf8_next(&s) : 0;
			unsigned int x0 = (c->col + k) * cw, y0 = (c->row + r) * ch;

			if (!model_render(cp, cw, ch, want)) {
				why = "the check names a codepoint with no model";
				break;
			}
			for (unsigned int i = 0; i < cw * ch && !why; i++) {
				uint32_t p = fb[(y0 + i / cw) * fb_w + x0 + i % cw];
				uint32_t *ref = want[i] ? &on : &off;
				bool *seen = want[i] ? &seen_on : &seen_off;

				if (!*seen) {
					*ref = p;
					*seen = true;
				} else if (p != *ref) {
					why = bad[!!want[i]];
				}
			}
		}
	}
	if (!why && *s)
		why = "the check names more codepoints than it has cells";
	if (!why && seen_on && seen_off && on == off)
		why = "ink and background are the same colour";
	free(want);
	return why;
}

/* @s's checks against the buffer last drawn, @cw x @ch cells; 0 if all hold */
static int run_checks(const struct screen *s, unsigned int cw, unsigned int ch)
{
	uint32_t bg = fb[fb_w * fb_h - 1];
	int ret = 0;

	for (const struct check *c = s->checks; c && c->kind; c++) {
		unsigned int x0 = c->col * cw, x1 = (c->col + c->cols) * cw;
		unsigned int y0 = c->row * ch, y1 = (c->row + c->rows) * ch;

		if (c->kind == 'b' || c->kind == 'c' || c->kind == 'm' || c->kind == 'p') {
			const char *why = c->kind == 'p' ? check_mosaic(c, cw, ch)
							 : check_box(c->kind, x0, y0, x1, y1);

			if (why) {
				fprintf(stderr,
					"test_pixels: %s: check %c on cells %u,%u +%ux%u fails: "
					"%s\n",
					s->name, c->kind, c->col, c->row, c->cols, c->rows, why);
				ret = -EINVAL;
			}
			continue;
		}
		unsigned int outer0 = c->kind == 'h' ? x0 : y0, outer1 = c->kind == 'h' ? x1 : y1;
		uint32_t first = fb[y0 * fb_w + x0];

		for (unsigned int a = outer0; a < outer1; a++) {
			unsigned int inner0 = c->kind == 'h' ? y0 : x0;
			unsigned int inner1 = c->kind == 'h' ? y1 : x1;
			bool ink = false, bad = false;

			for (unsigned int b = inner0; b < inner1; b++) {
				unsigned int x = c->kind == 'h' ? a : b, y = c->kind == 'h' ? b : a;
				uint32_t p = fb[y * fb_w + x];

				ink |= p != bg;
				bad |= c->kind == 'f' && (p != first || p == bg);
			}
			if (bad || !ink) {
				fprintf(stderr,
					"test_pixels: %s: check %c on cells %u,%u +%ux%u fails at "
					"pixel %s %u%s\n",
					s->name, c->kind, c->col, c->row, c->cols, c->rows,
					c->kind == 'h' ? "column" : "row", a,
					bad ? " (not one solid colour)" : " (background only)");
				ret = -EINVAL;
				break;
			}
		}
	}
	return ret;
}

/*
 * bbulk's compose() as 6ca1445 wrote it, one loop that tests the format at
 * every pixel, kept to judge the loop per format: both write the same bytes.
 */
static void compose_ref(struct video_buffer *dst, const struct video_buffer *src, unsigned int ox,
			unsigned int oy, struct tsm_screen_color c)
{
	bool argb = src->format == VIDEO_FORMAT_ARGB32;
	unsigned int x, y, w, h;
	uint32_t *d;

	if (ox >= dst->width || oy >= dst->height)
		return;
	w = min(src->width, dst->width - ox);
	h = min(src->height, dst->height - oy);

	for (y = 0; y < h; y++) {
		d = &((uint32_t *)dst->data)[(oy + y) * dst->width + ox];
		for (x = 0; x < w; x++) {
			uint32_t a, na, p = d[x], sr, sg, sb;

			if (argb) {
				uint32_t s = ((const uint32_t *)src->data)[y * src->width + x];

				a = s >> 24;
				sr = (s >> 16) & 0xff;
				sg = (s >> 8) & 0xff;
				sb = s & 0xff;
			} else {
				a = src->data[y * src->width + x];
				sr = over_div255(c.r * a);
				sg = over_div255(c.g * a);
				sb = over_div255(c.b * a);
			}
			if (!a)
				continue;
			na = 255 - a;
			d[x] = ((a + over_div255((p >> 24) * na)) << 24) |
			       ((sr + over_div255(((p >> 16) & 0xff) * na)) << 16) |
			       ((sg + over_div255(((p >> 8) & 0xff) * na)) << 8) |
			       (sb + over_div255((p & 0xff) * na));
		}
	}
}

static uint32_t xorshift(uint32_t *s)
{
	*s ^= *s << 13;
	*s ^= *s >> 17;
	*s ^= *s << 5;
	return *s;
}

/*
 * compose() and compose_ref() paint @src over the same backgrounds, in each of
 * a few colours, at offsets that place it whole, clipped on the right and
 * bottom, and wholly outside: 0 if every byte agrees.
 */
static int compose_matches(const char *name, const struct video_buffer *src)
{
	static const struct tsm_screen_color colours[] = {
		{.r = 0, .g = 0, .b = 0},	{.r = 255, .g = 255, .b = 255},
		{.r = 200, .g = 150, .b = 100}, {.r = 1, .g = 254, .b = 127},
		{.r = 128, .g = 64, .b = 32},
	};
	unsigned int dw = src->width + 3, dh = src->height + 2;
	unsigned int offs[][2] = {{0, 0},	    {1, 1},  {3, 2}, {dw - 2, 1},
				  {dw - 2, dh - 2}, {dw, 0}, {0, dh}};
	size_t size = (size_t)dw * dh * 4;
	struct video_buffer *a = malloc(sizeof(*a) + size), *b = malloc(sizeof(*b) + size);
	uint32_t seed = 0x9e3779b9;
	int ret = 0;

	assert(a && b);
	a->width = b->width = dw;
	a->height = b->height = dh;
	a->format = b->format = VIDEO_FORMAT_ARGB32;
	for (size_t c = 0; c < sizeof(colours) / sizeof(colours[0]); c++)
		for (size_t o = 0; o < sizeof(offs) / sizeof(offs[0]); o++)
			for (int bg = 0; bg < 3; bg++) {
				/* clear, as a default background is; opaque; anything */
				for (size_t i = 0; i < size / 4; i++)
					((uint32_t *)a->data)[i] = bg == 0   ? 0
								   : bg == 1 ? 0xff203040
									     : xorshift(&seed);
				memcpy(b->data, a->data, size);
				compose(a, src, offs[o][0], offs[o][1], colours[c]);
				compose_ref(b, src, offs[o][0], offs[o][1], colours[c]);
				if (memcmp(a->data, b->data, size)) {
					fprintf(stderr,
						"test_pixels: %s: compose() of a %ux%u %s buffer "
						"at %u,%u differs from the reference\n",
						name, src->width, src->height,
						src->format == VIDEO_FORMAT_ARGB32 ? "ARGB" : "A8",
						offs[o][0], offs[o][1]);
					ret = -EINVAL;
					goto out;
				}
			}
out:
	free(a);
	free(b);
	return ret;
}

/*
 * Every glyph and sprite the screen left in bbulk's caches, and a mask and a
 * colour buffer that hold every alpha, composed both ways: 0 if they agree.
 */
static int check_compose(struct kmscon_text *txt, const char *name)
{
	struct bbulk *bb = txt->data;
	struct shl_lru_entry *entry;
	struct htable_iter it;
	struct video_buffer *every;
	unsigned int n = 0;
	uint32_t seed = 1;
	int ret = 0;

	for (entry = htable_first(&bb->glyphs->tbl, &it); entry && !ret;
	     entry = htable_next(&bb->glyphs->tbl, &it), n++)
		ret = compose_matches(name, &((struct kmscon_glyph *)entry->value)->buf);
	for (int i = 0; i < KMSCON_DECO_COUNT && !ret; i++) {
		if (!bb->sprites[i])
			continue;
		ret = compose_matches(name, &bb->sprites[i]->buf);
		n++;
	}
	if (ret)
		return ret;
	if (!n) {
		fprintf(stderr, "test_pixels: %s: no glyph or sprite to compose\n", name);
		return -EINVAL;
	}

	every = malloc(sizeof(*every) + 16 * 16 * 4);
	assert(every);
	every->width = 16;
	every->height = 16;
	every->format = VIDEO_FORMAT_A8;
	for (unsigned int i = 0; i < 256; i++)
		every->data[i] = i;
	ret = compose_matches(name, every);
	every->format = VIDEO_FORMAT_ARGB32;
	for (unsigned int i = 0; i < 256 && !ret; i++) {
		uint32_t r = xorshift(&seed);

		/* premultiplied: no channel above alpha */
		((uint32_t *)every->data)[i] = i << 24 | (r & 0xff) * i / 255 << 16 |
					       (r >> 8 & 0xff) * i / 255 << 8 |
					       (r >> 16 & 0xff) * i / 255;
	}
	if (!ret)
		ret = compose_matches(name, every);
	free(every);
	return ret;
}

static void draw_frame(struct kmscon_text *txt, struct tsm_screen *con, struct tsm_vte *vte,
		       const struct kmscon_conf_t *conf)
{
	struct tsm_screen_attr attr;
	struct kmscon_text_deco deco;
	uint32_t *tmp;
	int ret;

	if (conf) {
		kmscon_conf_resolve_deco(conf, vte, &deco);
		kmscon_text_set_decorations(txt, &deco);
	}
	tsm_vte_get_def_attr(vte, &attr);
	ret = kmscon_text_prepare(txt, &attr, false);
	assert(ret == 0);
	ret = kmscon_text_draw(txt, con, false);
	assert(ret == 0);
	ret = kmscon_text_render(txt);
	assert(ret == 0);

	/* swap: the next frame draws into the other buffer */
	tmp = fb;
	fb = fb_back;
	fb_back = tmp;
	fake_d2d.rb[1].map = fb;
}

/*
 * After incremental frames: the buffer last drawn equals what a new renderer,
 * drawing every cell of the screen as it is now, draws. 0 if it does.
 */
static int check_fresh(struct kmscon_font *font, struct tsm_screen *con, struct tsm_vte *vte,
		       const struct screen *s, const char *osd)
{
	size_t size = (size_t)fb_w * fb_h * sizeof(*fb);
	uint32_t *last = malloc(size);
	struct kmscon_text *txt;
	int ret = 0;

	assert(last);
	memcpy(last, fb_back, size);
	assert(kmscon_text_new(&txt, "bbulk", s->orientation, &fake_disp) == 0);
	assert(kmscon_text_set(txt, font) == 0);
	assert(kmscon_text_set_osd(txt, osd) == 0);
	memset(fb, 0, size);
	draw_frame(txt, con, vte, s->conf);
	if (memcmp(fb_back, last, size)) {
		fprintf(stderr,
			"test_pixels: %s: the incremental frames differ from a fresh draw\n",
			s->name);
		ret = -EINVAL;
	}
	kmscon_text_unref(txt);
	free(last);
	return ret;
}

static int render_screen(struct kmscon_font *font, const struct screen *s, const char *out_dir)
{
	struct tsm_screen *con;
	struct tsm_vte *vte;
	struct kmscon_text *txt;
	char path[4096];
	int ret;

	ret = tsm_screen_new(&con, NULL, NULL);
	assert(ret == 0);
	ret = tsm_vte_new(&vte, con, vte_write, NULL, NULL, NULL);
	assert(ret == 0);
	tsm_vte_set_palette(vte, "default");
	ret = tsm_screen_resize(con, s->cols ? s->cols : COLS, s->rows ? s->rows : ROWS);
	assert(ret == 0);
	/* placements are measured in the font's cells, as kmscon tells libtsm */
	assert(tsm_screen_set_cell_size(con, font->width, font->height) == 0);
	tsm_screen_set_image_limit(con, 64 << 20);
	tsm_vte_set_sixel(vte, true);
	tsm_vte_set_kitty_graphics(vte, true);

	/* hidden cursor: its blink state is not part of what is compared */
	tsm_vte_input(vte, ESC "[?25l", 6);
	tsm_vte_input(vte, s->bytes, strlen(s->bytes));

	ret = kmscon_text_new(&txt, "bbulk", s->orientation, &fake_disp);
	assert(ret == 0);
	ret = kmscon_text_set(txt, font);
	assert(ret == 0);
	assert(txt->cols == (s->cols ? s->cols : COLS) && txt->rows == (s->rows ? s->rows : ROWS));

	assert(kmscon_text_set_osd(txt, s->osd) == 0);
	if (s->images)
		s->images(con, vte, 0, font->width, font->height);
	draw_frame(txt, con, vte, s->conf);
	if (s->next) {
		if (s->images)
			s->images(con, vte, 1, font->width, font->height);
		draw_frame(txt, con, vte, s->conf);
		tsm_vte_input(vte, s->next, strlen(s->next));
		if (s->osd_next)
			assert(kmscon_text_set_osd(txt, s->osd_next) == 0);
		if (s->images)
			s->images(con, vte, 2, font->width, font->height);
		draw_frame(txt, con, vte, s->conf);
		if (s->images)
			s->images(con, vte, 3, font->width, font->height);
		draw_frame(txt, con, vte, s->conf);
		/* both buffers now hold the last frame */
		if (memcmp(fb, fb_back, (size_t)fb_w * fb_h * sizeof(*fb))) {
			fprintf(stderr,
				"test_pixels: %s: the two buffers differ after the last frame\n",
				s->name);
			ret = -EINVAL;
		}
		/* and what a fresh draw of the last screen draws: nothing stale */
		if (!ret)
			ret = check_fresh(font, con, vte, s, s->osd_next ? s->osd_next : s->osd);
	}

	/* the buffer last drawn */
	snprintf(path, sizeof(path), "%s/%s.ppm", out_dir, s->name);
	fb = fb_back;
	if (!ret)
		ret = write_ppm(path);
	if (ret)
		fprintf(stderr, "test_pixels: cannot write %s: %s\n", path, strerror(-ret));

	snprintf(path, sizeof(path), "%s/%s.txt", out_dir, s->name);
	if (!ret)
		ret = write_text(path, s->desc);
	if (ret)
		fprintf(stderr, "test_pixels: cannot write %s: %s\n", path, strerror(-ret));

	if (!ret)
		ret = run_checks(s, font->width, font->height);
	if (!ret)
		ret = check_compose(txt, s->name);
	fb = fake_d2d.rb[1].map;

	kmscon_text_unref(txt);
	tsm_vte_unref(vte);
	tsm_screen_unref(con);
	return ret;
}

static void check_font_files(struct kmscon_font *font, const char *font_dir);

/* Replace the process's fontconfig with one that knows only the pinned files. */
static void pin_fonts(const char *font_dir)
{
	static const char *const files[] = {"DejaVuSansMono.ttf", "DejaVuSansMono-Bold.ttf",
					    "NotoSansMonoCJKJP-Subset.otf",
					    "NotoColorEmoji-Subset.ttf"};
	FcConfig *cfg = FcConfigCreate();
	char path[4096];

	assert(cfg);
	for (size_t i = 0; i < sizeof(files) / sizeof(files[0]); i++) {
		snprintf(path, sizeof(path), "%s/%s", font_dir, files[i]);
		if (!FcConfigAppFontAddFile(cfg, (const FcChar8 *)path)) {
			fprintf(stderr, "test_pixels: cannot load font %s\n", path);
			exit(2);
		}
	}
	if (!FcConfigSetCurrent(cfg)) {
		fprintf(stderr, "test_pixels: cannot install the font configuration\n");
		exit(2);
	}
	/* the current configuration holds its own reference, so the next pin frees this one */
	FcConfigDestroy(cfg);
}

/* Every font the backend can draw from, regular and bold, must be a pinned file. */
static void check_font_files(struct kmscon_font *font, const char *font_dir)
{
	struct ft_data *ftd = font->data;
	FcFontSet *sets[2] = {ftd->regular.fc, ftd->bold.fc};
	char dir[PATH_MAX], file[PATH_MAX];

	if (!realpath(font_dir, dir)) {
		fprintf(stderr, "test_pixels: bad font dir %s\n", font_dir);
		exit(2);
	}
	for (int k = 0; k < 2; k++) {
		assert(sets[k] && sets[k]->nfont > 0);
		for (int i = 0; i < sets[k]->nfont; i++) {
			FcChar8 *path = NULL;

			if (FcPatternGetString(sets[k]->fonts[i], FC_FILE, 0, &path) !=
				    FcResultMatch ||
			    !realpath((char *)path, file) || strncmp(file, dir, strlen(dir)) != 0 ||
			    file[strlen(dir)] != '/') {
				fprintf(stderr, "test_pixels: font %s is not under %s\n",
					path ? (char *)path : "(none)", dir);
				exit(2);
			}
		}
	}
}

/*
 * Draw @s with the pinned font at its size and line spacing, into a
 * framebuffer of its size in cells, COLS x ROWS by default.
 */
static int run_screen(const struct screen *s, const char *font_dir, const char *out_dir)
{
	struct kmscon_font *font = NULL;
	int ret;

	/* a fresh configuration per font: the backend's font set filter, set
	 * again by a second font, drops fonts added to the configuration by
	 * file */
	pin_fonts(font_dir);
	ret = kmscon_font_find(&font, "DejaVu Sans Mono", s->px ? s->px : FONT_PX, "freetype");
	if (ret || !font || font->ops != &kmscon_font_freetype_ops) {
		fprintf(stderr, "test_pixels: pinned font not found\n");
		exit(2);
	}
	check_font_files(font, font_dir);
	font->height += s->spacing;
	font->emoji_color = !s->no_emoji_color;

	fb_w = (s->cols ? s->cols : COLS) * font->width;
	fb_h = (s->rows ? s->rows : ROWS) * font->height;
	if (s->orientation == OR_RIGHT || s->orientation == OR_LEFT) {
		unsigned int w = fb_w;

		fb_w = fb_h;
		fb_h = w;
	}
	fb = calloc((size_t)fb_w * fb_h, sizeof(*fb));
	fb_back = calloc((size_t)fb_w * fb_h, sizeof(*fb));
	assert(fb && fb_back);
	fake_display_init();

	ret = render_screen(font, s, out_dir);

	kmscon_font_unref(font);
	free(fb);
	free(fb_back);
	return ret;
}

int main(int argc, char **argv)
{
	int ret, failed = 0;

	if (argc != 3) {
		fprintf(stderr, "usage: %s FONT_DIR OUT_DIR\n", argv[0]);
		return 2;
	}

	ret = kmscon_font_register(&kmscon_font_freetype_ops);
	assert(ret == 0);
	ret = kmscon_text_register(&kmscon_text_bbulk_ops);
	assert(ret == 0);

	for (size_t i = 0; i < sizeof(screens) / sizeof(screens[0]); i++)
		failed |= !!run_screen(&screens[i], argv[1], argv[2]);

	return failed ? 1 : 0;
}
