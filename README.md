# MultiK
## Rust Multi - Kernel project
Developed for educational purposes (for me to learn more about how OSs work)
This is a simple multi kernel designed with some ideas I had. It has heavy inspirations from early linux versions, as well as Philipp Oppermann's [blog-os](https://os.phil-opp.com/).
Thanks to the limine team for the bootloader, and also the limine-rs team for the project template, because I don't know heck about makefiles.

## Current state

This is still heavily a work in progress. I did make a more simple version, with a monolithic kernel, which works and you can find it in "kernel/src_old". That one has much more inspiration from phil-opp's blog os, but I did try to add some stuff myself, and kinda failed (the most notable one being trying to use my own page table, which I couldn't achieve without any bugs, and I also had some trouble with the logic behind it).
But there's still a small shell, also, since the VGA buffer doesn't work, I'm using my own font, which I import from a font file. Credits for that font goes to Jonathan Gray, over on [OpenBSD fonts](https://openports.eu/ports/fonts/tamsyn-font)
