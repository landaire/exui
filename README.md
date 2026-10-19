# exui

- Decompiles Xbox 360 XUI scenes (`.xur`) and string tables (`.xus`) to XML.
- Compiles that XML, or XUI files you write yourself, back to the binary forms.
- Unpacks and packs XUIZ archives.
- Runs XUI scenes in your own program with the `exui-runtime` library.

## Usage

Decompile an archive into a directory of `.xui` scenes, `.xus.xml` string tables and the other files:

```sh
exui decompile dashmain -o dashmain-src --order-file dashmain.order
```

Edit the files, then build the archive again:

```sh
exui pack dashmain-src -o dashmain --order dashmain.order
```

Without `--order` the entries are sorted by name. Compile a single scene or string table with `exui compile scene.xui` or `exui compile strings.xus.xml`. `exui extract` and `exui list` work on archives without decompiling.

A decompiled scene compiles back to the same bytes. The text rules that make this work are in [doc/xui-format.md](doc/xui-format.md).

## Libraries

[`exui-core`](crates/exui-core) reads XUIZ archives and XUR scenes. Use it if you only need the file formats.

[`exui-runtime`](crates/exui-runtime) instantiates scenes, runs their timelines, focus and messages, lays out text and renders each frame to an [`xbox360-draw`](https://github.com/landaire/rust-xbox360) draw list. It is `no_std` and runs on the console and on a PC. You supply the archives and a renderer. [`xbox360-raster`](https://github.com/landaire/rust-xbox360) is a software renderer for that draw list.

```rust
let mut ui = exui_runtime::Ui::new(registry, &archives, display);
ui.load_scene("section://0,main#", "main.xur")?;
loop {
    ui.update(frame_time);
    ui.draw(&mut renderer, &mut draw_list)?;
}
```

[`exui-extension`](crates/exui-extension) is the descriptor format of extension archives, which add scenes and patch existing ones without changing the base archives.

## Building

```sh
cargo build --release
```
