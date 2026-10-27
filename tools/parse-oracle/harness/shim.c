/*
 * The shim between the C reference's parser and the Rust side (src/c_side.rs).
 *
 * build.rs extracts the parser by line range from the pinned libtsm commit
 * (pin.tsv, extract.tsv) into the *.inc files included here, unchanged. This
 * file adds only what the extracted code needs to compile and to reach Rust:
 *
 * - the fields of struct tsm_vte the parser reads, mirrored in src/c_side.rs
 *   and checked against oracle_layout when the Rust side starts;
 * - each dispatch the parser makes (print, execute, ESC, CSI, OSC end,
 *   XTGETTCAP, APC) and its pause check, sent to a Rust callback;
 * - each function the departures act on, renamed ref_* as it is included,
 *   with its calls sent to a Rust hook that calls back oracle_ref_*.
 *
 * Logging, the charset mapping, the sixel handler and the image fast paths
 * compile to nothing.
 */

#include <errno.h>
#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>
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

/* The Rust side's callbacks and hooks (src/c_side.rs). */
void oracle_print(struct tsm_vte *vte, uint32_t c);
void oracle_execute(struct tsm_vte *vte, uint32_t c);
void oracle_esc(struct tsm_vte *vte, uint32_t final);
void oracle_csi(struct tsm_vte *vte, uint32_t final);
void oracle_osc_end(struct tsm_vte *vte, uint32_t end);
void oracle_xtgettcap(struct tsm_vte *vte);
void oracle_apc_start(struct tsm_vte *vte);
void oracle_apc_put(struct tsm_vte *vte, uint32_t c);
void oracle_apc_end(struct tsm_vte *vte, uint32_t end);
bool oracle_paused(struct tsm_vte *vte);
void oracle_warning(void);
void oracle_hook_clear(struct tsm_vte *vte);
void oracle_hook_collect(struct tsm_vte *vte, uint32_t data);
void oracle_hook_param(struct tsm_vte *vte, uint32_t data);
void oracle_hook_osc_collect(struct tsm_vte *vte, uint32_t c);
void oracle_hook_dcs_start(struct tsm_vte *vte, uint32_t data);
void oracle_hook_dcs_collect(struct tsm_vte *vte, uint32_t data);
void oracle_hook_dcs_end(struct tsm_vte *vte, uint32_t data);
void oracle_hook_action(struct tsm_vte *vte, uint32_t data, int action);
void oracle_hook_parse(struct tsm_vte *vte, uint32_t raw);

#define llog_debug(obj, ...) ((void)(obj))
#define llog_warning(obj, ...) oracle_warning()
#define tsm_symbol_make(ucs4) (ucs4)
#define vte_map(vte, val) (val)
#define write_console oracle_print
#define do_execute oracle_execute
#define do_esc oracle_esc
#define do_csi oracle_csi
#define do_osc_end oracle_osc_end
#define dcs_xtgettcap oracle_xtgettcap
#define vte_apc_start oracle_apc_start
#define vte_apc_collect oracle_apc_put
#define vte_apc_end oracle_apc_end
#define vte_image_paused oracle_paused
#define vte_image_cancel(vte) ((void)(vte))
#define vte_dcs_start(vte, data) ((void)(vte))
#define vte_dcs_collect(vte, data) ((void)(vte))
#define vte_dcs_end(vte, data) ((void)(vte))
#define vte_sixel_run(vte, u8, len) ((size_t)0)
#define vte_kitty_run(vte, u8, len) ((size_t)0)

#define do_clear ref_do_clear
#include "do-clear.inc"
#undef do_clear
#define do_clear oracle_hook_clear

#define do_collect ref_do_collect
#include "do-collect.inc"
#undef do_collect
#define do_collect oracle_hook_collect

#define do_param ref_do_param
#include "do-param.inc"
#undef do_param
#define do_param oracle_hook_param

#define do_osc_collect ref_do_osc_collect
#include "do-osc-collect.inc"
#undef do_osc_collect
#define do_osc_collect oracle_hook_osc_collect

#define do_dcs_start ref_do_dcs_start
#define do_dcs_collect ref_do_dcs_collect
#define do_dcs_end ref_do_dcs_end
#include "dcs.inc"
#undef do_dcs_start
#undef do_dcs_collect
#undef do_dcs_end
#define do_dcs_start oracle_hook_dcs_start
#define do_dcs_collect oracle_hook_dcs_collect
#define do_dcs_end oracle_hook_dcs_end

#define do_action ref_do_action
#include "do-action.inc"
#undef do_action
#define do_action oracle_hook_action

#include "transitions.inc"

#define parse_data ref_parse_data
#include "parse-data.inc"
#undef parse_data
#define parse_data oracle_hook_parse

#include "vte-input.inc"

/* What the Rust side calls. */

void oracle_ref_clear(struct tsm_vte *vte) { ref_do_clear(vte); }
void oracle_ref_collect(struct tsm_vte *vte, uint32_t d) { ref_do_collect(vte, d); }
void oracle_ref_param(struct tsm_vte *vte, uint32_t d) { ref_do_param(vte, d); }
void oracle_ref_osc_collect(struct tsm_vte *vte, uint32_t c) { ref_do_osc_collect(vte, c); }
void oracle_ref_dcs_start(struct tsm_vte *vte, uint32_t d) { ref_do_dcs_start(vte, d); }
void oracle_ref_dcs_collect(struct tsm_vte *vte, uint32_t d) { ref_do_dcs_collect(vte, d); }
void oracle_ref_dcs_end(struct tsm_vte *vte, uint32_t d) { ref_do_dcs_end(vte, d); }
void oracle_ref_action(struct tsm_vte *vte, uint32_t d, int a) { ref_do_action(vte, d, a); }
void oracle_ref_parse(struct tsm_vte *vte, uint32_t raw) { ref_parse_data(vte, raw); }
void oracle_trans(struct tsm_vte *vte, uint32_t d, int s, int a) { do_trans(vte, d, s, a); }
size_t oracle_input(struct tsm_vte *vte, const char *u8, size_t len) { return vte_input(vte, u8, len); }

/* the parser's part of tsm_vte_reset (tsm-vte.c) */
void oracle_reset(struct tsm_vte *vte)
{
#include "reset.inc"
}

struct tsm_vte *oracle_new(void)
{
	struct tsm_vte *vte = calloc(1, sizeof(*vte));

	if (!vte || tsm_utf8_mach_new(&vte->mach)) {
		free(vte);
		return NULL;
	}
	vte->state = STATE_GROUND;
	vte->work_left = 1;
	return vte;
}

void oracle_free(struct tsm_vte *vte)
{
	if (vte)
		tsm_utf8_mach_free(vte->mach);
	free(vte);
}

/* The constants and the layout the Rust side mirrors, in src/c_side.rs's
 * order, checked when it starts. */
const uint32_t oracle_constants[] = {
	CSI_ARG_MAX, CSI_SUBARG_MAX, OSC_MAX_LEN, XTGETTCAP_MAX_LEN,
	TSM_VTE_FLAG_8BIT_MODE, TSM_VTE_FLAG_7BIT_MODE, CSI_OTHER,
	DCS_IGNORED, DCS_XTGETTCAP,
	ACTION_NONE, ACTION_OSC_START, ACTION_DCS_END, ACTION_OSC_END,
	ACTION_APC_END,
	STATE_GROUND, STATE_ESC, STATE_ESC_INT, STATE_CSI_ENTRY,
	STATE_CSI_PARAM, STATE_CSI_INT, STATE_CSI_IGNORE, STATE_DCS_ENTRY,
	STATE_DCS_PARAM, STATE_DCS_INT, STATE_DCS_IGNORE,
};

#define FIELD(name) offsetof(struct tsm_vte, name)
const size_t oracle_layout[] = {
	sizeof(struct tsm_vte),
	FIELD(mach), FIELD(state), FIELD(dcs), FIELD(dcs_esc), FIELD(csi_argc),
	FIELD(csi_argv), FIELD(csi_subargc), FIELD(csi_subargv),
	FIELD(csi_subarg_drop), FIELD(csi_flags), FIELD(osc_len),
	FIELD(osc_arg), FIELD(tcap_buf), FIELD(tcap_len), FIELD(tcap_over),
	FIELD(flags), FIELD(sync_frames), FIELD(sync_stop), FIELD(work_left),
};
