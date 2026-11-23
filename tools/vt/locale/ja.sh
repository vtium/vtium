# shellcheck shell=sh
# Japanese mode, one of vtium's locale mode scripts. Source it in a vtium
# console, as in `. ja.sh`, to switch that console and its shell to Japanese.
#
# The script switches the console's font set to noto-cjk-jp, which draws kana,
# and kanji in the forms Japanese uses, with Noto Sans CJK JP. The switch goes
# through `vtium-config use-frit`, which sends OSC 50 and checks that vtium
# took the font set. Then the script sets LANG to ja_JP.UTF-8 in the shell
# that sources it. A program cannot change the environment of the shell that
# started it, so the script is sourced rather than run. When vtium does not
# take the font set, the script leaves LANG as it was and returns 1. It sets
# no other variable and changes nothing else in the shell.
#
# vtium sets the keyboard layout when it starts. While it runs, only a change
# to the whole system's layout through localed changes it, in a build with
# logind. So the script names the Japanese layout and says how to set it.
#
# References:
# - ADR-1511 D3: what a locale mode script switches, and that it returns.

if vtium-config use-frit noto-cjk-jp; then
	export LANG=ja_JP.UTF-8
	echo 'Japanese mode: LANG is ja_JP.UTF-8, and the font set is noto-cjk-jp.'
	echo 'The keyboard layout stays as it was, because vtium sets the layout when it starts.'
	echo 'For the Japanese layout, set xkb-layout = jp in vtium.conf and start vtium again.'
	[ -z "${LC_ALL:-}" ] || echo 'LC_ALL is set, and it overrides LANG. Unset LC_ALL to use the Japanese locale.'
else
	echo 'Japanese mode failed: vtium did not switch to the font set noto-cjk-jp, so LANG stays as it was.' >&2
	return 1
fi
