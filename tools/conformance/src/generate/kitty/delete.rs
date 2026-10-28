//! Deleting placements and images: each delete target, lowercase and
//! uppercase, and the scrollback.

use super::{BLUE4, GREEN, RED, kitty, kt, rgba};
use crate::generate::sixel::{BLUE, six_at};
use crate::generate::{Case, Inputs, Made, cell, cup, sized};

pub(in crate::generate) const CASES: &[Case] = &[
    // a and A: every placement on the screen; A frees what it leaves
    // unplaced; image 3 is placed again before A so A has something to do
    sized("kit-delete-a", 12, 4, |_| {
        delete_case(
            &kitty("a=d,d=a", b""),
            &[kitty("a=p,i=2,C=1", b""), kitty("a=d,d=A", b"")].concat(),
        )
        .into()
    }),
    sized("kit-delete-i", 12, 4, delete_i),
    sized("kit-delete-n", 12, 4, delete_n),
    // c and C: the placements under the cursor
    sized("kit-delete-c", 12, 4, |_| {
        delete_case(
            &[cup(1, 2), kitty("a=d,d=c", b"")].concat(),
            &[cup(1, 6), kitty("a=d,d=C", b"")].concat(),
        )
        .into()
    }),
    // p and P: the placements over cell x, y (1-based)
    sized("kit-delete-p", 12, 4, |_| {
        delete_case(
            &kitty("a=d,d=p,x=2,y=1", b""),
            &kitty("a=d,d=P,x=5,y=1", b""),
        )
        .into()
    }),
    // q and Q: over a cell and with a z; a wrong z deletes nothing
    sized("kit-delete-q", 12, 4, |_| {
        delete_case(
            &[
                kitty("a=d,d=q,x=1,y=1,z=1", b""),
                kitty("a=d,d=Q,x=9,y=1,z=1", b""),
            ]
            .concat(),
            &kitty("a=d,d=Q,x=6,y=1,z=2", b""),
        )
        .into()
    }),
    // r and R: images whose id lies in x to y; an inverted range deletes
    // nothing
    sized("kit-delete-r", 12, 4, |_| {
        delete_case(
            &[kitty("a=d,d=r,x=1,y=1", b""), kitty("a=d,d=R,x=3,y=1", b"")].concat(),
            &kitty("a=d,d=R,x=2,y=2", b""),
        )
        .into()
    }),
    // x and X: the placements over a column (1-based)
    sized("kit-delete-x", 12, 4, |_| {
        delete_case(&kitty("a=d,d=x,x=2", b""), &kitty("a=d,d=X,x=5", b"")).into()
    }),
    // y and Y: the placements over a row (1-based); image 1 is moved to
    // row 3 and image 2 to row 4 first, its placement id replacing them
    sized("kit-delete-y", 12, 4, |_| {
        delete_case(
            &[
                cup(3, 1),
                kitty("a=p,i=1,p=1,C=1", b""),
                kitty("a=d,d=y,y=3", b""),
            ]
            .concat(),
            &[
                cup(4, 5),
                kitty("a=p,i=2,p=1,C=1", b""),
                kitty("a=d,d=Y,y=4", b""),
            ]
            .concat(),
        )
        .into()
    }),
    // z and Z: the placements with a z
    sized("kit-delete-z", 12, 4, |_| {
        delete_case(&kitty("a=d,d=z,z=1", b""), &kitty("a=d,d=Z,z=2", b"")).into()
    }),
    // f and F: animation frames, which do not exist yet: nothing goes
    sized("kit-delete-f", 12, 4, |_| {
        delete_case(&kitty("a=d,d=f,i=1", b""), &kitty("a=d,d=F,i=2", b"")).into()
    }),
    sized("kit-delete-sb", 12, 4, delete_sb),
];

/// Images 1, 2 and 3 placed at columns 0, 4 and 8, with placement ids 1;
/// `lower` should remove image 1's placement and `upper` image 2's, and
/// placing each again shows which kept its data (OK) and which lost it
/// (ENOENT); image 3 stays.
fn delete_case(lower: &str, upper: &str) -> String {
    [
        &cell(8, 16),
        &kitty("a=T,f=32,s=16,v=16,i=1,p=1,z=1,C=1", &rgba(16, 16, RED)),
        &cup(1, 5),
        &kitty("a=T,f=32,s=16,v=16,i=2,p=1,z=2,C=1", &rgba(16, 16, GREEN)),
        &cup(1, 9),
        &kitty("a=T,f=32,s=16,v=16,i=3,p=1,z=3,C=1", &rgba(16, 16, BLUE4)),
        lower,
        upper,
        &cup(4, 1),
        &kitty("a=p,i=1,C=1", b""),
        &cup(4, 5),
        &kitty("a=p,i=2,C=1", b""),
    ]
    .concat()
}

/// i and I, by id, and with p only that placement; an uppercase delete
/// whose p matches none of an image's placements frees nothing, even an
/// image with no placement at all (image 4, placed again at the end).
fn delete_i(_: &Inputs) -> Made {
    let lower = [
        kitty("a=t,f=32,s=8,v=8,i=4", &rgba(8, 8, [255, 0, 255, 255])),
        kitty("a=d,d=i,i=1", b""),
    ]
    .concat();
    let upper = [
        kitty("a=d,d=I,i=2,p=1", b""),
        kitty("a=d,d=I,i=3,p=2", b""),
        kitty("a=d,d=I,i=4,p=9", b""),
    ]
    .concat();
    (delete_case(&lower, &upper) + &cup(4, 9) + &kitty("a=p,i=4,C=1", b"")).into()
}

/// n and N: the newest image with the number.
fn delete_n(_: &Inputs) -> Made {
    [
        cell(8, 16),
        kitty("a=T,f=32,s=16,v=16,I=4,C=1", &rgba(16, 16, RED)),
        cup(1, 5),
        kitty("a=T,f=32,s=16,v=16,I=4,C=1", &rgba(16, 16, GREEN)),
        cup(1, 9),
        kitty("a=T,f=32,s=16,v=16,I=6,C=1", &rgba(16, 16, BLUE4)),
        kitty("a=d,d=n,I=4", b""),
        kitty("a=d,d=N,I=6", b""),
        cup(4, 1),
        kitty("a=p,i=2,C=1", b""),
        cup(4, 5),
        kitty("a=p,i=3,C=1", b""),
    ]
    .concat()
    .into()
}

/// a deletes only on the screen and I in scrollback too: images 1 and 3
/// scroll into scrollback, and the view shows that a left image 1 and I
/// took image 3; neither touches a sixel.
fn delete_sb(_: &Inputs) -> Made {
    [
        &cell(8, 16),
        &tt!("sb-max", n = 10),
        &kt(1, 8, 16, RED, ",C=1"),
        &cup(1, 4),
        &kt(3, 8, 16, RED, ",C=1"),
        &cup(4, 1),
        "\n",
        &kt(2, 8, 16, RED, ",C=1"),
        &six_at(4, 2, 8, 16, BLUE),
        &kitty("a=d,d=a", b""),
        &kitty("a=d,d=I,i=3", b""),
        &tt!("sb-up", n = 1),
    ]
    .concat()
    .into()
}
