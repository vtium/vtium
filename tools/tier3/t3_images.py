"""The images variants' checks of the image helper's accounts before anyone
logs in (ADR-1101 D2, D10): the prelogin case.

At the login prompt the session's pty belongs to root, and vtium, built
without logind, refuses file media for a pty owned by root and decodes
inline pictures as the account vtium-image, or as nobody when that account
does not exist. The agent (guest/t3-agent, `prelogin`) writes two kitty
pictures to the login prompt's pty as a program there would: the quarters
PNG read from a file at row 12, column 5, and the same PNG sent inline at
row 12, column 25. The case passes when the pty is root's, the image
helpers that run are the variant's pre-login account's, the file's picture
is not drawn, and the inline picture is drawn pixel for pixel. It is a
check without a golden: its capture, the login prompt with the inline
picture, is kept when it passes.
"""

import t3_pictures

CASE = "prelogin"
CHECKS = {
    CASE: "Before anyone logs in, two kitty pictures written to the login prompt's pty: the "
          "quarters PNG read from a file at row 12, column 5, and the same PNG sent inline at "
          "row 12, column 25 (t3_images.py). Correct: the inline picture alone, the image "
          "helper running as the variant's pre-login account, vtium-image or nobody, and "
          "nothing where the file's picture would be, since vtium reads no file for a pty "
          "that root owns (ADR-1101 D2).",
}
# Where the agent places the two pictures, as (column, row) counted from 1.
FILE_AT = (5, 12)
INLINE_AT = (25, 12)
# The variant's cell, the 8x16 test font's.
CELL = (8, 16)


def parse(answer):
    """The agent's answer to `prelogin` as {"pty": (path, owner), "helpers":
    [account...]}, or {"refused": why}."""
    out = {"pty": None, "helpers": None}
    for line in answer:
        word, _, rest = line.partition(" ")
        if line.startswith("refused"):
            return {"refused": line}
        if word == "pty":
            path, _, owner = rest.partition(" ")
            out["pty"] = (path, owner)
        elif word == "helpers":
            out["helpers"] = [] if rest == "none" else rest.split()
    return out


def drawn(img, at, picture="quad"):
    """How many of `picture`'s opaque pixels `img` shows in their colors with
    the picture's top left corner at cell `at`, and how many there are."""
    pic = t3_pictures.PICTURES[picture]
    left, top = (at[0] - 1) * CELL[0], (at[1] - 1) * CELL[1]
    shown = total = 0
    for y, row in enumerate(pic.pixels):
        for x, p in enumerate(row):
            if p:
                total += 1
                i = ((top + y) * img.width + left + x) * 3
                shown += tuple(img.data[i:i + 3]) == t3_pictures.eight(p)
    return shown, total


def judge(answer, img, account):
    """Why the prelogin case fails, from the agent's `answer` and the
    capture `img`, for a variant whose pre-login account is `account`; None
    when it passes."""
    got = parse(answer)
    if "refused" in got:
        return "the agent answered %s" % got["refused"]
    if got["pty"] is None or got["pty"][1] != "root":
        return "the login prompt's pty is %s, not root's" % (got["pty"],)
    if not got["helpers"] or set(got["helpers"]) != {account}:
        return ("the image helpers before login run as %s; they must run as %s"
                % (" ".join(got["helpers"] or ["nobody at all"]), account))
    shown, total = drawn(img, INLINE_AT)
    if shown != total:
        return "the inline picture shows %d of its %d pixels" % (shown, total)
    shown, _ = drawn(img, FILE_AT)
    if shown:
        return ("the picture read from a file shows %d pixels, so vtium read a file for a "
                "pty that root owns" % shown)
    return None
