# Images

vtium shows images sent with sixel, the kitty graphics protocol, or iTerm2's inline images. It plays kitty graphics animations and renders the protocol's Unicode placeholders. Each image is anchored to a line of text and scrolls with it.

A separate program, the image helper, reads and decodes every image inside a sandbox, so vtium's root process never decodes an image format itself. [Security and the system](security-and-system.md#helpers-in-a-sandbox) says which user it runs as and how it is locked down.

Images need a build with the `images` feature, which the `vtium` and `vtium-git` packages include. [Images](../features.md#images) covers each protocol in detail.
