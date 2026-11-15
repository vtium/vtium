# Images

vtium shows images sent with sixel, the kitty graphics protocol, or iTerm2's inline images. It plays kitty graphics animations and renders the protocol's Unicode placeholders. Each image is anchored to a line of text and scrolls with it.

A separate program, the image helper, reads and decodes every image. It runs as your user, never as root, inside a sandbox.

More in [Images](../features.md#images) in What vtium does: which builds show images, and the details of each protocol.
