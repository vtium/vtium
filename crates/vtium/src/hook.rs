//! The panic hook. vtium's profiles abort on a panic, which runs no
//! guard, so the hook puts the VT back before the panic is printed: the
//! message then lands on a console in text mode with its keyboard.

/// Sets a panic hook that runs `restore`, then the hook in place before
/// it, which prints the panic.
pub fn restore_first(restore: impl Fn() + Send + Sync + 'static) {
    let print = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        restore();
        print(info);
    }));
}
