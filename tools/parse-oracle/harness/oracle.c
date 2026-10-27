/*
 * The C reference's parser, driven by an event script and writing one trace
 * line per action, in the format the Rust side (src/main.rs) writes.
 *
 * The parser is the C reference's own code: oracle.py extracts it by line
 * range from the pinned libtsm commit (pin.tsv, extract.tsv) into the
 * *.inc files this file includes, unchanged. This file adds what the
 * extracted code calls: the parser-only fields of struct tsm_vte, a stub
 * for each dispatch that prints its trace line, and the switches that apply
 * vtium-parse's departures (departures.tsv), each named on the command line.
 * With no switch named, the trace is the C reference's.
 *
 * usage: oracle-c [departure-id ...] < script > trace
 *
 * The script and the trace are described in this directory's README.
 */

#include <errno.h>
#include <limits.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

/* libtsm.h, libtsm-int.h and tsm-vte-int.h: the constants the parser uses */
#include "ucs4-replacement.inc"
#include "symbol.inc"
#include "input-flags.inc"
#include "termcap-name.inc"
#include "utf8-states.inc"
#include "bounds.inc"

/* tsm-unicode.c: the encoder OSC strings use, and the UTF-8 decoder */
#include "ucs4-to-utf8.inc"
#include "utf8-mach.inc"

/* tsm-vte.c: the states, actions, intermediate flags and DCS kinds */
#include "parser-enums.inc"

/* The fields of struct tsm_vte (tsm-vte-int.h) the extracted code reads. */
struct tsm_vte {
	struct tsm_utf8_mach *mach;
	unsigned int state;
	unsigned int dcs;
	bool dcs_esc;
	unsigned int csi_argc;
	int csi_argv[CSI_ARG_MAX];
	unsigned int csi_subargc[CSI_ARG_MAX];
	int csi_subargv[CSI_ARG_MAX][CSI_SUBARG_MAX];
	bool csi_subarg_drop;
	unsigned int csi_flags;
	unsigned int osc_len;
	char osc_arg[OSC_MAX_LEN];
	char tcap_buf[XTGETTCAP_MAX_LEN];
	size_t tcap_len;
	bool tcap_over;
	unsigned int flags;
	uint64_t sync_frames;
	bool sync_stop;
	uint64_t work_left;
};

/* vtium-parse's departures, in departures.tsv's order */
enum departure {
	D_STRING_END,
	D_INTERMEDIATES,
	D_RESET_APC_ST,
	D_EMPTY_PARAMS,
	D_C1_UTF8,
	D_GR_AS_GL,
	D_PAST_ASCII,
	D_OSC_PREFIX,
	D_NUM,
};

static const char *const departure_ids[D_NUM] = {
	[D_STRING_END] = "string-end",
	[D_INTERMEDIATES] = "intermediates",
	[D_RESET_APC_ST] = "reset-apc-st",
	[D_EMPTY_PARAMS] = "empty-params",
	[D_C1_UTF8] = "c1-utf8",
	[D_GR_AS_GL] = "gr-as-gl",
	[D_PAST_ASCII] = "past-ascii",
	[D_OSC_PREFIX] = "osc-prefix",
};

/* What the oracle keeps beside the C reference's state. */
static struct {
	/* the departures applied, one bit each */
	unsigned int departs;

	/* the bytes the sequence in progress collected: its private marker,
	 * or -1, and its intermediates, the first two kept */
	int marker;
	uint32_t ims[2];
	unsigned int nims;
	/* a character failed to fit the OSC string in progress */
	bool osc_truncated;
	/* the DCS string in progress reaches the trace (every one but
	 * XTGETTCAP: vtium-parse hands the choice of handler to its consumer,
	 * where the C reference made it at do_dcs_start) */
	bool hooked;

	/* string-end: the exit action an ESC deferred, or ACTION_NONE, with
	 * the OSC string the ESC's own clear wiped */
	int pending;
	char pending_osc[OSC_MAX_LEN];
	unsigned int pending_osc_len;
	bool pending_osc_truncated;
	/* reset-apc-st: the next character is dropped if it is '\' */
	bool drop_backslash;

	/* the script's triggers: a pause (and a reset at it) after so many
	 * counted lines, and a mode switch after so many dispatches */
	unsigned long pause_left;
	bool reset_at_pause;
	bool paused;
	unsigned long mode_left;
	char mode_to;
} o;

static bool departs(enum departure d)
{
	return o.departs & 1u << d;
}

static void set_mode(struct tsm_vte *vte, char mode)
{
	vte->flags &= ~(TSM_VTE_FLAG_7BIT_MODE | TSM_VTE_FLAG_8BIT_MODE);
	if (mode == '8')
		vte->flags |= TSM_VTE_FLAG_8BIT_MODE;
	else if (mode == '7')
		vte->flags |= TSM_VTE_FLAG_7BIT_MODE;
	printf("mode %c\n", mode);
}

/* Ends a trace line. A counted line moves the pause trigger on; a dispatch
 * line (ESC or CSI) moves the mode trigger on. */
static void line_end(struct tsm_vte *vte, bool counted, bool dispatch)
{
	putchar('\n');
	if (counted && o.pause_left && !--o.pause_left)
		o.paused = true;
	if (dispatch && o.mode_left && !--o.mode_left)
		set_mode(vte, o.mode_to);
}

static void print_end(uint32_t end)
{
	if (end == 0x9c)
		printf("st");
	else if (end == 0x07)
		printf("bel");
	else
		printf("cancel:%x", end);
}

/* The sequence's private marker and intermediates: the bytes in order with
 * intermediates applied, the C reference's flag set otherwise. */
static void print_ims(struct tsm_vte *vte)
{
	unsigned int i;

	if (!departs(D_INTERMEDIATES)) {
		if (vte->csi_flags)
			printf(" f%x", vte->csi_flags);
		else
			printf(" -");
		return;
	}
	if (o.marker < 0 && !o.nims) {
		printf(" -");
		return;
	}
	putchar(' ');
	if (o.marker >= 0)
		printf("%02x", o.marker);
	for (i = 0; i < o.nims && i < 2; ++i)
		printf("%02x", o.ims[i]);
}

/* The parameters, as many as do_csi counts (tsm-vte.c:2542-2543). */
static void print_params(struct tsm_vte *vte)
{
	unsigned int argc, i, j;

	argc = vte->csi_argc < CSI_ARG_MAX ? vte->csi_argc + 1 : CSI_ARG_MAX;
	printf(" [");
	for (i = 0; i < argc; ++i) {
		if (i)
			putchar(',');
		printf("%d", vte->csi_argv[i]);
		for (j = 0; j < vte->csi_subargc[i]; ++j)
			printf(":%d", vte->csi_subargv[i][j]);
	}
	putchar(']');
}

/* intermediates: a third intermediate makes the sequence do nothing */
static bool null_operation(void)
{
	return departs(D_INTERMEDIATES) && o.nims > 2;
}

/* What the extracted code calls outside the parser: a trace line for each
 * dispatch, and nothing for logging, charsets and images. */

#define llog_debug(obj, ...) ((void)(obj))
#define llog_warning(obj, ...) (printf("warning\n"))
#define tsm_symbol_make(ucs4) (ucs4)
#define vte_sixel_run(vte, u8, len) ((size_t)0)
#define vte_kitty_run(vte, u8, len) ((size_t)0)
#define vte_image_paused(vte) (o.paused)

static void vte_image_cancel(struct tsm_vte *vte)
{
	(void)vte;
}

/* the charset mapping is the consumer's in vtium */
static uint32_t vte_map(struct tsm_vte *vte, uint32_t val)
{
	(void)vte;
	return val;
}

static void write_console(struct tsm_vte *vte, tsm_symbol_t sym)
{
	printf("print %x", sym);
	line_end(vte, true, false);
}

static void do_execute(struct tsm_vte *vte, uint32_t ctrl)
{
	printf("exec %x", ctrl);
	line_end(vte, true, false);
}

static void do_esc(struct tsm_vte *vte, uint32_t data)
{
	if (null_operation())
		return;
	printf("esc %x", data);
	print_ims(vte);
	line_end(vte, true, true);
}

static void do_csi(struct tsm_vte *vte, uint32_t data)
{
	if (null_operation())
		return;
	printf("csi %x", data);
	print_ims(vte);
	print_params(vte);
	line_end(vte, true, true);
}

static void do_osc_end(struct tsm_vte *vte, uint32_t val)
{
	unsigned int i;

	printf("osc ");
	print_end(val);
	printf(o.osc_truncated ? " t " : " ");
	for (i = 0; i < vte->osc_len; ++i)
		printf("%02x", (uint8_t)vte->osc_arg[i]);
	line_end(vte, true, false);
}

static void dcs_xtgettcap(struct tsm_vte *vte)
{
	size_t i;

	if (vte->tcap_over) {
		printf("tcap none");
	} else {
		printf("tcap ");
		for (i = 0; i < vte->tcap_len; ++i)
			printf("%02x", (uint8_t)vte->tcap_buf[i]);
	}
	line_end(vte, true, false);
}

/* sixel's handler: the trace is written at do_dcs_* below */
static void vte_dcs_start(struct tsm_vte *vte, uint32_t data)
{
	(void)vte;
	(void)data;
}

static void vte_dcs_collect(struct tsm_vte *vte, uint32_t data)
{
	(void)vte;
	(void)data;
}

static void vte_dcs_end(struct tsm_vte *vte, uint32_t data)
{
	(void)vte;
	(void)data;
}

static void vte_apc_start(struct tsm_vte *vte)
{
	printf("apc_start");
	line_end(vte, true, false);
}

static void vte_apc_collect(struct tsm_vte *vte, uint32_t data)
{
	printf("apc_put %x", data);
	line_end(vte, false, false);
}

static void vte_apc_end(struct tsm_vte *vte, uint32_t data)
{
	printf("apc_end ");
	print_end(data);
	line_end(vte, true, false);
}

/* The extracted parser. A function the oracle wraps is renamed ref_* as it
 * is included, and the wrapper, which calls it, takes its name. */

#define do_clear ref_do_clear
#include "do-clear.inc"
#undef do_clear

static void do_clear(struct tsm_vte *vte)
{
	ref_do_clear(vte);
	o.marker = -1;
	o.nims = 0;
}

#define do_collect ref_do_collect
#include "do-collect.inc"
#undef do_collect

static void do_collect(struct tsm_vte *vte, uint32_t data)
{
	if (data >= 0x3c && data <= 0x3f) {
		o.marker = data;
	} else {
		if (o.nims < 2)
			o.ims[o.nims] = data;
		if (o.nims < UINT_MAX)
			++o.nims;
	}
	ref_do_collect(vte, data);
}

#define do_param ref_do_param
#include "do-param.inc"
#undef do_param

static void do_param(struct tsm_vte *vte, uint32_t data)
{
	unsigned int i = vte->csi_argc;
	bool empty = data == ';' && i < CSI_ARG_MAX && vte->csi_argv[i] == -1;

	ref_do_param(vte, data);
	/* empty-params: the C reference made it 0 */
	if (departs(D_EMPTY_PARAMS) && empty)
		vte->csi_argv[i] = -1;
}

#define do_osc_collect ref_do_osc_collect
#include "do-osc-collect.inc"
#undef do_osc_collect

static void do_osc_collect(struct tsm_vte *vte, uint32_t val)
{
	char buf[4];
	unsigned int len = vte->osc_len;

	/* osc-prefix: after a character that failed to fit, nothing */
	if (departs(D_OSC_PREFIX) && o.osc_truncated)
		return;
	ref_do_osc_collect(vte, val);
	/* a noncharacter encodes to nothing and is no truncation */
	if (vte->osc_len == len && tsm_ucs4_to_utf8(val, buf))
		o.osc_truncated = true;
}

#define do_dcs_start ref_do_dcs_start
#define do_dcs_collect ref_do_dcs_collect
#define do_dcs_end ref_do_dcs_end
#include "dcs.inc"
#undef do_dcs_start
#undef do_dcs_collect
#undef do_dcs_end

static void do_dcs_start(struct tsm_vte *vte, uint32_t data)
{
	o.hooked = false;
	if (null_operation()) {
		vte->dcs = DCS_IGNORED;
		return;
	}
	/* intermediates: XTGETTCAP is one '+', where the flag set also took
	 * two */
	if (departs(D_INTERMEDIATES) && o.nims > 1)
		vte->csi_flags |= CSI_OTHER;
	ref_do_dcs_start(vte, data);
	if (vte->dcs == DCS_XTGETTCAP)
		return;
	o.hooked = true;
	printf("dcs_hook %x", data);
	print_ims(vte);
	print_params(vte);
	line_end(vte, true, false);
}

static void do_dcs_collect(struct tsm_vte *vte, uint32_t data)
{
	if (o.hooked) {
		printf("dcs_put %x", data);
		line_end(vte, false, false);
	}
	ref_do_dcs_collect(vte, data);
}

static void do_dcs_end(struct tsm_vte *vte, uint32_t data)
{
	if (o.hooked) {
		printf("dcs_unhook ");
		print_end(data);
		line_end(vte, true, false);
	}
	o.hooked = false;
	ref_do_dcs_end(vte, data);
}

#define do_action ref_do_action
#include "do-action.inc"
#undef do_action

static void do_action(struct tsm_vte *vte, uint32_t data, int action)
{
	if (action == ACTION_OSC_START)
		o.osc_truncated = false;

	/* string-end: an ESC ends no string until the next character */
	if (departs(D_STRING_END) && data == 0x1b &&
	    (action == ACTION_OSC_END || action == ACTION_DCS_END ||
	     action == ACTION_APC_END)) {
		o.pending = action;
		memcpy(o.pending_osc, vte->osc_arg, sizeof(o.pending_osc));
		o.pending_osc_len = vte->osc_len;
		o.pending_osc_truncated = o.osc_truncated;
		return;
	}
	ref_do_action(vte, data, action);
}

#include "transitions.inc"

#define parse_data ref_parse_data
#include "parse-data.inc"
#undef parse_data

/* the states of a sequence's header, where gr-as-gl applies */
static bool in_header(unsigned int state)
{
	switch (state) {
	case STATE_ESC:
	case STATE_ESC_INT:
	case STATE_CSI_ENTRY:
	case STATE_CSI_PARAM:
	case STATE_CSI_INT:
	case STATE_CSI_IGNORE:
	case STATE_DCS_ENTRY:
	case STATE_DCS_PARAM:
	case STATE_DCS_INT:
		return true;
	}
	return false;
}

static void parse_data(struct tsm_vte *vte, uint32_t raw)
{
	bool seven = vte->flags & TSM_VTE_FLAG_7BIT_MODE;
	bool eight = !seven && (vte->flags & TSM_VTE_FLAG_8BIT_MODE);
	bool utf8 = !seven && !eight;

	if (departs(D_C1_UTF8) && utf8 && raw >= 0x80 && raw <= 0x9f)
		return;

	if (departs(D_GR_AS_GL) && eight && raw >= 0xa0 && raw <= 0xff &&
	    in_header(vte->state))
		raw -= 0x80;

	if (o.pending) {
		int action = o.pending;

		o.pending = ACTION_NONE;
		if (action == ACTION_OSC_END) {
			memcpy(vte->osc_arg, o.pending_osc, sizeof(o.pending_osc));
			vte->osc_len = o.pending_osc_len;
			o.osc_truncated = o.pending_osc_truncated;
		}
		ref_do_action(vte, raw == '\\' ? 0x9c : 0x1b, action);
	}

	if (o.drop_backslash) {
		o.drop_backslash = false;
		if (raw == '\\')
			return;
	}

	if (departs(D_PAST_ASCII) && utf8 && raw >= 0xa0) {
		/* the character still ends an XTGETTCAP request's wait for
		 * '\', as ref_parse_data's first lines end it for every
		 * character (tsm-vte.c:3282-3287) */
		vte->dcs_esc = false;
		switch (vte->state) {
		case STATE_ESC:
		case STATE_ESC_INT:
			do_trans(vte, raw, STATE_GROUND, ACTION_NONE);
			return;
		case STATE_DCS_ENTRY:
		case STATE_DCS_PARAM:
		case STATE_DCS_INT:
			do_trans(vte, raw, STATE_DCS_IGNORE, ACTION_NONE);
			return;
		}
	}

	ref_parse_data(vte, raw);
}

#include "vte-input.inc"

/* The parser's part of tsm_vte_reset; the mode stays the consumer's. */
static void reset(struct tsm_vte *vte)
{
#include "reset.inc"
	o.hooked = false;
	o.drop_backslash = departs(D_RESET_APC_ST) && o.pending == ACTION_APC_END;
	o.pending = ACTION_NONE;
	printf("reset\n");
}

static void stream_start(struct tsm_vte *vte, unsigned long n, char mode)
{
	unsigned int departs = o.departs;

	free(vte->mach);
	memset(vte, 0, sizeof(*vte));
	if (tsm_utf8_mach_new(&vte->mach)) {
		fprintf(stderr, "oracle-c: out of memory\n");
		exit(2);
	}
	vte->state = STATE_GROUND;
	vte->work_left = 1;
	memset(&o, 0, sizeof(o));
	o.departs = departs;
	o.marker = -1;
	printf("stream %lu\n", n);
	set_mode(vte, mode);
}

/* Feeds one data record whole, resuming after each pause. */
static void feed(struct tsm_vte *vte, const unsigned char *data, size_t len)
{
	size_t off = 0;

	while (off < len) {
		off += vte_input(vte, (const char *)data + off, len - off);
		if (o.paused) {
			o.paused = false;
			printf("pause %zu\n", off);
			if (o.reset_at_pause)
				reset(vte);
		} else if (off < len) {
			fprintf(stderr, "oracle-c: input stopped without a pause\n");
			exit(2);
		}
	}
}

static unsigned char *read_all(FILE *f, size_t *len)
{
	size_t cap = 1 << 16, n = 0, got;
	unsigned char *buf = malloc(cap);

	while (buf && (got = fread(buf + n, 1, cap - n, f)) > 0) {
		n += got;
		if (n == cap)
			buf = realloc(buf, cap *= 2);
	}
	if (!buf) {
		fprintf(stderr, "oracle-c: out of memory\n");
		exit(2);
	}
	*len = n;
	return buf;
}

static void truncated(void)
{
	fprintf(stderr, "oracle-c: the script ends inside a record\n");
	exit(2);
}

static uint32_t read_u32(const unsigned char *p)
{
	return p[0] | p[1] << 8 | p[2] << 16 | (uint32_t)p[3] << 24;
}

int main(int argc, char **argv)
{
	static char outbuf[1 << 16];
	struct tsm_vte vte = { 0 };
	unsigned long streams = 0;
	unsigned char *script;
	size_t len, i = 0;
	uint32_t n;
	int a, d;

	for (a = 1; a < argc; ++a) {
		for (d = 0; d < D_NUM; ++d)
			if (!strcmp(argv[a], departure_ids[d]))
				break;
		if (d == D_NUM) {
			fprintf(stderr, "oracle-c: no departure '%s'\n", argv[a]);
			return 2;
		}
		o.departs |= 1u << d;
	}

	setvbuf(stdout, outbuf, _IOFBF, sizeof(outbuf));
	script = read_all(stdin, &len);
	while (i < len) {
		char kind = script[i++];

		switch (kind) {
		case 'S':
			if (len - i < 1)
				truncated();
			stream_start(&vte, streams++, script[i++]);
			break;
		case 'D':
			if (len - i < 4 || len - i - 4 < read_u32(script + i))
				truncated();
			n = read_u32(script + i);
			feed(&vte, script + i + 4, n);
			i += 4 + n;
			break;
		case 'R':
			reset(&vte);
			break;
		case 'M':
			if (len - i < 1)
				truncated();
			set_mode(&vte, script[i++]);
			break;
		case 'P':
		case 'Q':
			if (len - i < 4)
				truncated();
			o.pause_left = read_u32(script + i);
			o.reset_at_pause = kind == 'Q';
			i += 4;
			break;
		case 'T':
			if (len - i < 5)
				truncated();
			o.mode_left = read_u32(script + i);
			o.mode_to = script[i + 4];
			i += 5;
			break;
		default:
			fprintf(stderr, "oracle-c: no record kind '%c'\n", kind);
			return 2;
		}
	}
	free(vte.mach);
	free(script);
	return fflush(stdout) ? 2 : 0;
}
