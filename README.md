# exui

- Decompiles Xbox 360 XUI scenes (`.xur`) and string tables (`.xus`) to XML.
- Compiles that XML, or XUI files you write yourself, back to the binary forms.
- Unpacks and packs XUIZ archives.

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

## Building

```sh
cargo build --release
```
