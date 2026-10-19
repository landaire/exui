//! Compiler tests over synthetic scenes and string tables.

use std::fmt::Write;
use std::fs;

use exui::compile::CompiledScene;
use exui::compile::compile_scene;
use exui::pack;
use exui::pack::Conversion;
use exui::pack::SourceContents;
use exui::pack::SourceKind;
use exui::registry;
use exui::scene::Scene;
use exui::xui::Xml;
use exui::xus::Entries;
use exui::xus::IdEntry;
use exui::xus::NameEntry;
use exui::xus::StringKey;
use exui::xus::StringTable;
use exui::xus::StringTableXml;
use exui::xus::compile_string_table;
use exui_core::AcceptedVersions;
use exui_core::Archive;
use exui_core::Registry;
use exui_core::tree::Object;
use exui_core::tree::Value;

/// Every value type, a compound, an indexed property, figure points with
/// and without a stored size, a name declared at two class levels, named
/// frames with each command and timelines with compound and indexed paths,
/// in the form the decompiler writes.
const SCENE: &str = r#"<XuiCanvas version="000c" allocationhints="true">
<Properties>
<Width>640.000000</Width>
<Height>480.000000</Height>
</Properties>
<XuiFigure>
<Properties>
<Id>Shape</Id>
<Width>70.000000</Width>
<Height>25.500000</Height>
<Position>1.000000,-2.000000,0.000000</Position>
<Rotation>0.000000,0.000000,0.707107,0.707107</Rotation>
<Opacity>0.0000001</Opacity>
<Anchor>4294967295</Anchor>
<Show>false</Show>
<Fill>
<Properties>
<FillColor>0xff102030</FillColor>
<TextureFileName>a b  &#xD;
c&lt;&amp;&gt;	 </TextureFileName>
<Gradient>
<Properties>
<NumStops>-2</NumStops>
<StopColor index="0">0x00000000</StopColor>
<StopColor index="1">0xffffffff</StopColor>
<StopPos index="0">0.000000</StopPos>
<StopPos index="1">1.000000</StopPos>
</Properties>
</Gradient>
</Properties>
</Fill>
<Closed>true</Closed>
<Points>2,0.000000,0.000000,0.000000,0.000000,70.000000,0.000000,0,70.000000,25.500000,70.000000,25.500000,0.000000,25.500000,0,</Points>
</Properties>
</XuiFigure>
<XuiFigure>
<Properties>
<Id>Line</Id>
<Stroke>
</Stroke>
<Points width="10.000000" height="20.000000">1,1.000000,2.000000,3.000000,4.000000,5.000000,6.000000,0,</Points>
</Properties>
</XuiFigure>
<XuiEffect>
<Properties>
<Id>Outer</Id>
<Id>Inner</Id>
</Properties>
</XuiEffect>
<Timelines>
<NamedFrames>
<NamedFrame>
<Name>Start</Name>
<Time>0</Time>
</NamedFrame>
<NamedFrame>
<Name>Hold</Name>
<Time>10</Time>
<Command>stop</Command>
</NamedFrame>
<NamedFrame>
<Name>Jump</Name>
<Time>20</Time>
<Command>goto</Command>
<CommandParams>Start</CommandParams>
</NamedFrame>
<NamedFrame>
<Name>Loop</Name>
<Time>30</Time>
<Command>gotoandplay</Command>
<CommandParams>Start</CommandParams>
</NamedFrame>
<NamedFrame>
<Name>End</Name>
<Time>40</Time>
<Command>gotoandstop</Command>
<CommandParams>Hold</CommandParams>
</NamedFrame>
<NamedFrame>
<Time>50</Time>
<Command>7</Command>
<CommandParams>End</CommandParams>
</NamedFrame>
</NamedFrames>
<Timeline>
<Id>Shape</Id>
<TimelineProp>Opacity</TimelineProp>
<TimelineProp>Fill.FillColor</TimelineProp>
<TimelineProp index="1">Fill.Gradient.StopColor</TimelineProp>
<TimelineProp>Closed</TimelineProp>
<KeyFrame>
<Time>0</Time>
<Interpolation>0</Interpolation>
<Prop>1.000000</Prop>
<Prop>0xff000000</Prop>
<Prop>0x80808080</Prop>
<Prop>true</Prop>
</KeyFrame>
<KeyFrame>
<Time>15</Time>
<Interpolation>1</Interpolation>
<EaseScale>30</EaseScale>
<Prop>0.500000</Prop>
<Prop>0xff00ff00</Prop>
<Prop>0x00000000</Prop>
<Prop>false</Prop>
</KeyFrame>
<KeyFrame>
<Time>40</Time>
<Interpolation>2</Interpolation>
<EaseIn>10</EaseIn>
<EaseOut>20</EaseOut>
<EaseScale>50</EaseScale>
<Prop>0.000000</Prop>
<Prop>0xffff0000</Prop>
<Prop>0xffffffff</Prop>
<Prop>true</Prop>
</KeyFrame>
</Timeline>
<Timeline>
<Id>Inner Id</Id>
</Timeline>
</Timelines>
</XuiCanvas>
"#;

/// Classes, properties and timelines of `object` and its descendants, one
/// line each.
fn describe(root: &Object, registry: &Registry) -> String {
    let mut out = String::new();
    let mut stack = vec![(root, 0)];
    while let Some((object, depth)) = stack.pop() {
        let indent = "  ".repeat(depth);
        let class = registry.class(object.class).expect("registered").name();
        writeln!(out, "{indent}{class}").expect("string write");
        for property in &object.properties {
            writeln!(
                out,
                "{indent}  {} = {}",
                property.def.name,
                describe_values(&property.values)
            )
            .expect("string write");
        }
        for frame in &object.named_frames {
            writeln!(
                out,
                "{indent}  frame {:?} at {} command {} target {:?}",
                frame.name, frame.time.0, frame.command.0, frame.target
            )
            .expect("string write");
        }
        for timeline in &object.timelines {
            writeln!(out, "{indent}  timeline {:?}", timeline.element).expect("string write");
            for property in &timeline.properties {
                let path = property.path.as_ref().map(|path| {
                    let names: Vec<&str> = path.defs.iter().map(|def| def.name).collect();
                    format!(
                        "up {} index {} {}",
                        path.levels_up.0,
                        path.index.0,
                        names.join(".")
                    )
                });
                writeln!(
                    out,
                    "{indent}    path {path:?} element {:?}",
                    property.element.map(|element| element.0)
                )
                .expect("string write");
            }
            for keyframe in &timeline.keyframes {
                writeln!(
                    out,
                    "{indent}    key {} interpolation {} ease {} {} {} {:?}",
                    keyframe.time.0,
                    keyframe.interpolation.0,
                    keyframe.ease_in.0,
                    keyframe.ease_out.0,
                    keyframe.ease_scale.0,
                    keyframe.values
                )
                .expect("string write");
            }
        }
        stack.extend(object.children.iter().rev().map(|child| (child, depth + 1)));
    }

    out
}

fn describe_values(values: &[Value]) -> String {
    let parts: Vec<String> = values
        .iter()
        .map(|value| match value {
            Value::Compound(properties) => {
                let parts: Vec<String> = properties
                    .iter()
                    .map(|property| {
                        format!(
                            "{} = {}",
                            property.def.name,
                            describe_values(&property.values)
                        )
                    })
                    .collect();
                format!("{{{}}}", parts.join(", "))
            }
            Value::Custom(blob) => format!("blob {blob:02x?}"),
            other => format!("{other:?}"),
        })
        .collect();

    parts.join(" | ")
}

fn compile_error(text: &str) -> String {
    let registry = registry::dash_6717().expect("registry builds");
    match compile_scene(text, &registry) {
        Ok(_) => "compiles".to_string(),
        Err(err) => err.to_string(),
    }
}

#[test]
fn synthetic_scene_round_trips() {
    let registry = registry::dash_6717().expect("registry builds");
    let compiled = CompiledScene::parse(SCENE, &registry).expect("compiles");
    let bytes = compiled.to_xur(&registry).expect("writes");

    let scene = Scene::parse(&bytes, &registry).expect("reads back");
    pretty_assertions::assert_eq!(Xml(&scene).to_string(), SCENE);
    assert!(compiled.options.allocation_hints);

    insta::assert_snapshot!(describe(&compiled.tree.root, &registry));
}

#[test]
fn xui_tool_conventions_compile() {
    // Line ends and indentation of hand-written text; values other than
    // strings may be surrounded by whitespace, and an indexed element
    // count, size attributes and ease values may be left out.
    let text = "<XuiCanvas version=\"000C\">\r\n  <Properties>\r\n    <Width> 64.5 </Width>\r\n  </Properties>\r\n  <XuiFigure>\r\n    <Properties>\r\n      <Id>A</Id>\r\n      <Points>1,2,3,4,5,6,7,1</Points>\r\n    </Properties>\r\n  </XuiFigure>\r\n  <Timelines>\r\n    <Timeline>\r\n      <Id>A</Id>\r\n      <TimelineProp>Width</TimelineProp>\r\n      <KeyFrame>\r\n        <Time>3</Time>\r\n        <Interpolation>1</Interpolation>\r\n        <Prop>\r\n 2 </Prop>\r\n      </KeyFrame>\r\n    </Timeline>\r\n  </Timelines>\r\n</XuiCanvas>\r\n";
    let registry = registry::dash_6717().expect("registry builds");
    let bytes = compile_scene(text, &registry).expect("compiles");
    let scene = Scene::parse(&bytes, &registry).expect("reads back");

    insta::assert_snapshot!(Xml(&scene).to_string());
}

#[test]
fn compile_errors() {
    let cases = [
        ("not XML", "<XuiCanvas version=\"000c\">"),
        ("no version", "<XuiCanvas/>"),
        ("old version", "<XuiCanvas version=\"000b\"/>"),
        (
            "bad hints",
            "<XuiCanvas version=\"000c\" allocationhints=\"1\"/>",
        ),
        (
            "unknown class",
            "<XuiCanvas version=\"000c\"><XuiNothing/></XuiCanvas>",
        ),
        (
            "unknown property",
            "<XuiCanvas version=\"000c\"><Properties><Size>1</Size></Properties></XuiCanvas>",
        ),
        (
            "repeated property",
            "<XuiCanvas version=\"000c\"><Properties><Width>1</Width><Width>2</Width></Properties></XuiCanvas>",
        ),
        (
            "bad float",
            "<XuiCanvas version=\"000c\"><Properties><Width>wide</Width></Properties></XuiCanvas>",
        ),
        (
            "bad vector",
            "<XuiCanvas version=\"000c\"><Properties><Position>1,2</Position></Properties></XuiCanvas>",
        ),
        (
            "bad color",
            "<XuiCanvas version=\"000c\"><XuiFigure><Properties><Stroke><Properties><StrokeColor>red</StrokeColor></Properties></Stroke></Properties></XuiFigure></XuiCanvas>",
        ),
        (
            "index out of order",
            "<XuiCanvas version=\"000c\"><XuiFigure><Properties><Fill><Properties><Gradient><Properties><StopPos index=\"0\">0</StopPos><StopPos index=\"2\">1</StopPos></Properties></Gradient></Properties></Fill></Properties></XuiFigure></XuiCanvas>",
        ),
        (
            "missing index",
            "<XuiCanvas version=\"000c\"><XuiFigure><Properties><Fill><Properties><Gradient><Properties><StopPos>0</StopPos></Properties></Gradient></Properties></Fill></Properties></XuiFigure></XuiCanvas>",
        ),
        (
            "index on a scalar",
            "<XuiCanvas version=\"000c\"><Properties><Width index=\"0\">1</Width></Properties></XuiCanvas>",
        ),
        (
            "point count",
            "<XuiCanvas version=\"000c\"><XuiFigure><Properties><Points>2,1,2,3,4,5,6,0,</Points></Properties></XuiFigure></XuiCanvas>",
        ),
        (
            "half a size",
            "<XuiCanvas version=\"000c\"><XuiFigure><Properties><Points width=\"1\">0,</Points></Properties></XuiFigure></XuiCanvas>",
        ),
        (
            "unknown command",
            "<XuiCanvas version=\"000c\"><Timelines><NamedFrames><NamedFrame><Time>0</Time><Command>jump</Command></NamedFrame></NamedFrames></Timelines></XuiCanvas>",
        ),
        (
            "frame without time",
            "<XuiCanvas version=\"000c\"><Timelines><NamedFrames><NamedFrame><Name>A</Name></NamedFrame></NamedFrames></Timelines></XuiCanvas>",
        ),
        (
            "unknown timeline target",
            "<XuiCanvas version=\"000c\"><XuiFigure/><Timelines><Timeline><Id>B</Id><TimelineProp>Width</TimelineProp></Timeline></Timelines></XuiCanvas>",
        ),
        (
            "unknown timeline property",
            "<XuiCanvas version=\"000c\"><XuiFigure><Properties><Id>A</Id></Properties></XuiFigure><Timelines><Timeline><Id>A</Id><TimelineProp>Fill.Size</TimelineProp></Timeline></Timelines></XuiCanvas>",
        ),
        (
            "keyframe value count",
            "<XuiCanvas version=\"000c\"><XuiFigure><Properties><Id>A</Id></Properties></XuiFigure><Timelines><Timeline><Id>A</Id><TimelineProp>Width</TimelineProp><KeyFrame><Time>0</Time><Interpolation>0</Interpolation></KeyFrame></Timeline></Timelines></XuiCanvas>",
        ),
        (
            "unexpected element",
            "<XuiCanvas version=\"000c\"><Timelines><Frames/></Timelines></XuiCanvas>",
        ),
        (
            "text in a container",
            "<XuiCanvas version=\"000c\">text</XuiCanvas>",
        ),
    ];

    let mut out = String::new();
    for (case, text) in cases {
        writeln!(out, "{case}: {}", compile_error(text)).expect("string write");
    }

    insta::assert_snapshot!(out);
}

fn tables() -> [StringTable; 3] {
    [
        StringTable {
            entries: Entries::Index(vec![
                "first".to_string(),
                String::new(),
                " two\r\nlines ".to_string(),
            ]),
        },
        StringTable {
            entries: Entries::Id(vec![
                IdEntry {
                    id: StringKey(7),
                    value: "<seven> & \"more\"".to_string(),
                },
                IdEntry {
                    id: StringKey(0xFFFF_FFFF),
                    value: "last\t".to_string(),
                },
            ]),
        },
        StringTable {
            entries: Entries::Name(vec![NameEntry {
                name: "key \"one\"\r\n\t".to_string(),
                value: "value".to_string(),
            }]),
        },
    ]
}

#[test]
fn string_tables_round_trip() {
    let mut out = String::new();
    for table in tables() {
        let bytes = table.to_bytes().expect("writes");
        let text = StringTableXml(&StringTable::parse(&bytes).expect("reads")).to_string();
        assert_eq!(StringTable::from_xml(&text).expect("compiles"), table);
        assert_eq!(compile_string_table(&text).expect("compiles"), bytes);
        out.push_str(&text);
    }

    insta::assert_snapshot!(out);
}

#[test]
fn string_table_errors() {
    let mut bytes = tables()[0].to_bytes().expect("writes");
    let mut out = String::new();

    let mut flags = bytes.clone();
    flags[5] = 3;
    let mut size = bytes.clone();
    size[9] ^= 1;
    bytes.push(0);
    let length = (bytes.len() as u32).to_be_bytes();
    bytes[6..10].copy_from_slice(&length);
    for (case, data) in [("flags", flags), ("size", size), ("trailing", bytes)] {
        let error = StringTable::parse(&data).expect_err("rejected");
        writeln!(out, "{case}: {error}").expect("string write");
    }

    for (case, text) in [
        ("no keys", "<XuiStringTable/>"),
        ("bad keys", "<XuiStringTable keys=\"hash\"/>"),
        (
            "bad id",
            "<XuiStringTable keys=\"id\"><String id=\"x\">a</String></XuiStringTable>",
        ),
        (
            "missing name",
            "<XuiStringTable keys=\"name\"><String>a</String></XuiStringTable>",
        ),
    ] {
        let error = StringTable::from_xml(text).expect_err("rejected");
        writeln!(out, "{case}: {error}").expect("string write");
    }

    insta::assert_snapshot!(out);
}

/// A decompiled directory packs back to the archive it came from, in the
/// recorded order or sorted by name.
#[test]
fn archive_packs_back() {
    let registry = registry::dash_6717().expect("registry builds");
    let scene = compile_scene(SCENE, &registry).expect("compiles");
    let table = tables()[1].to_bytes().expect("writes");
    let entries = [
        ("strings\\en-US.xus", table.as_slice()),
        ("Scene.xur", scene.as_slice()),
        ("image.png", b"not an image".as_slice()),
    ];
    let original = exui_core::archive_writer::write_archive(
        exui_core::ArchiveVersion::V1,
        &entries.map(|(name, data)| exui_core::archive_writer::NewEntry { name, data }),
    )
    .expect("writes");

    let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("archive_packs_back");
    if dir.exists() {
        fs::remove_dir_all(&dir).expect("stale directory is removable");
    }
    let archive = Archive::parse(&original, AcceptedVersions::ALL).expect("parses");
    let mut order = Vec::new();
    let mut kinds = Vec::new();
    for entry in archive.entries() {
        let name = entry.name().to_string();
        let data = archive.entry_data(&entry).expect("in bounds");
        let source = pack::source_file(&name, data, &registry);
        kinds.push((source.name.clone(), pack::entry_name(&source.name).1));
        let contents = match &source.contents {
            SourceContents::Text(text) => text.as_bytes(),
            SourceContents::Data { data, reason } => {
                assert!(reason.is_none());
                data
            }
        };
        let path = dir.join(source.name.replace('\\', std::path::MAIN_SEPARATOR_STR));
        fs::create_dir_all(path.parent().expect("parent")).expect("directory");
        fs::write(path, contents).expect("writes");
        order.push(name);
    }

    assert_eq!(
        kinds,
        [
            (
                "strings\\en-US.xus.xml".to_string(),
                SourceKind::StringTable
            ),
            ("Scene.xui".to_string(), SourceKind::Scene),
            ("image.png".to_string(), SourceKind::Raw),
        ]
    );

    let ordered =
        pack::pack_directory(&dir, Some(&order), Conversion::Compile, &registry).expect("packs");
    assert!(ordered == original);

    let sorted = pack::plan_directory(&dir, None, Conversion::Compile).expect("plans");
    let names: Vec<&str> = sorted.iter().map(|source| source.name.as_str()).collect();
    assert_eq!(names, ["Scene.xur", "image.png", "strings\\en-US.xus"]);

    let stored = pack::plan_directory(&dir, None, Conversion::Store).expect("plans");
    let names: Vec<&str> = stored.iter().map(|source| source.name.as_str()).collect();
    assert_eq!(names, ["Scene.xui", "image.png", "strings\\en-US.xus.xml"]);

    fs::remove_dir_all(&dir).expect("removable");
}
