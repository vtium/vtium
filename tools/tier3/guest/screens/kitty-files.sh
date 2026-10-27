#!/bin/bash
# kit: kitty
# look-for: A banner saying what to expect, then each reply marked. Logged in:
# look-for: the street at sunset three times, i=2, i=3 and i=4;OK, then
# look-for: /etc/shadow refused with i=5;EBADF, all "(expected)". In the VT
# look-for: demo (no XDG_SESSION_ID): "No login session", four EBADF "(expected here)".
# wrong-looks-like: Any line marked UNEXPECTED, a picture missing beside an OK,
# wrong-looks-like: a picture drawn under the "No login session" banner, or
# wrong-looks-like: anything but EBADF for /etc/shadow.
# screen-text: none, kitty pictures and cursor moves do not print as rows
# The kitty graphics protocol's file media through kmscon's image helper
# (ADR-103 step 4c), each with the terminal's reply: the photograph's PNG
# named by its path (t=f), read and left; a copy in /tmp named
# tty-graphics-protocol-* as the spec asks of a temporary file (t=t), read
# and deleted; a copy in a POSIX shared memory object (t=s), its name
# starting with / and holding no other / as the spec requires, read and
# unlinked; and /etc/shadow, which only root may read, named with t=f and
# refused with the spec's one read failure, EBADF. The helper reads as the
# session's user, so the pictures show in a session logged in through
# login, as tier 3's is and as docs/fork/demo.md's login run is. The VT
# demo's session is in no logind session of its own, which kmscon treats
# as before login (docs/image-helper.md): every file medium answers EBADF
# with no helper started, the temporary file and the object are left, and
# this page removes them. XDG_SESSION_ID decides which of the two the page
# expects: pam_systemd sets it in a login, and the VT demo's setpriv
# --reset-env clears it. The replies cannot decide it, since a missing or
# failed helper, or an identity bug, also answers EBADF in a login. A banner
# above the pictures says which, and each reply is marked expected or
# UNEXPECTED for it: in a login a refusal is UNEXPECTED, and without one an
# OK is, since the refusal before login failed. The page refuses to run as
# a user who can read /etc/shadow.
. "$(dirname "$(readlink -f "${BASH_SOURCE[0]}")")/kitty-common.inc"
kitty_offered
shadow=/etc/shadow
if [ -r "$shadow" ]; then
	printf 'This page runs as a user who can read %s; run it as an ordinary\n' "$shadow"
	printf 'user, for whom only root can read it.\n'
	exit 0
fi

# Prints a reply line and its mark: "(expected)", "(expected here)" without
# a login session, or UNEXPECTED with what was expected.
mark() {
	local line=$1 got=$2 want=$3 here=
	[ -n "$login" ] || here=' here'
	want=${want//\*;/}
	if [[ $got == $3 ]]; then
		printf '%s  (expected%s)\n' "$line" "$here"
	else
		printf '%s  UNEXPECTED, expected %s\n' "$line" "${want//\*/}"
	fi
}

top_row
printf 'The photograph from its file (t=f), a temporary file (t=t), shared memory (t=s):\n'
login=${XDG_SESSION_ID:+yes}
if [ -n "$login" ]; then
	printf 'Logged in: EXPECTED: three pictures with OK, then /etc/shadow refused with EBADF.\n'
else
	printf 'No login session (no XDG_SESSION_ID): kmscon refuses file images here by\n'
	printf 'design, so EXPECTED: four EBADF replies and no pictures. To see the pictures,\n'
	printf 'log in on a kmscon VT and run fork/demo/demo.sh kitty-files (fork/demo/README.md).\n'
fi
row=$((top + 5))
printf '%s[%d;1H' "$esc" "$row"
kitty_ask "a=T,i=2,t=f,f=100,C=1,c=20,r=8" "$(b64 "$photo_png")"
file_reply=$reply
temp=$(mktemp /tmp/tty-graphics-protocol-XXXXXX)
cp "$photo_png" "$temp"
printf '%s[%d;24H' "$esc" "$row"
kitty_ask "a=T,i=3,t=t,f=100,C=1,c=20,r=8" "$(b64 "$temp")"
temp_reply=$reply
temp_state=deleted
[ ! -e "$temp" ] || temp_state='still there'
rm -f "$temp"
shm=/kmscon-demo-$$-shm
cp "$photo_png" "/dev/shm$shm"
printf '%s[%d;47H' "$esc" "$row"
kitty_ask "a=T,i=4,t=s,f=100,C=1,c=20,r=8" "$(b64 "$shm")"
shm_reply=$reply
shm_state=unlinked
[ ! -e "/dev/shm$shm" ] || shm_state='still there'
rm -f "/dev/shm$shm"
kitty_ask "a=T,i=5,t=f,f=100,C=1" "$(b64 "$shadow")"
shadow_reply=$reply
printf '%s[%d;1H' "$esc" $((row + 8))
if [ -n "$login" ]; then
	ok='*;OK' gone=deleted unlinked=unlinked
else
	ok='*;EBADF*' gone='still there' unlinked='still there'
fi
mark "t=f: $file_reply" "$file_reply" "$ok"
mark "t=t: $temp_reply, temporary file $temp_state" \
	"$temp_reply, $temp_state" "$ok, $gone"
mark "t=s: $shm_reply, object $shm_state" "$shm_reply, $shm_state" "$ok, $unlinked"
mark "t=f $shadow (root only): $shadow_reply" "$shadow_reply" '*;EBADF*'
[ -n "$login" ] || printf 'The page removed the temporary file and the object kmscon left.\n'
