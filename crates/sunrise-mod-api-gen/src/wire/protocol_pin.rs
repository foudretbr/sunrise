//! The shared protocol's number guards what no declared shape covers. This test hashes the items
//! that define it, as the source writes them, so changing one without bumping
//! [`mod_surface::call::SHARED_PROTOCOL`] fails here.
use quote::ToTokens;
use std::path::Path;
use syn::Item;

/// The items whose text defines the protocol, by file below the repository root: the group-state
/// words in the std member, the handler and budget words, the static-reply bounds and the file
/// capability's root selector.
const ITEMS: &[(&str, &[&str])] = &[
    (
        "crates/sunrise-mod-api/src/wire/wire_guest.rs",
        &[
            "GROUP_WORDS",
            "WORD_HANDLERS",
            "WORD_BUDGET",
            "WORD_EVICTING",
            "WORD_EVICT",
            "take_static",
        ],
    ),
    (
        "crates/sunrise-mod-vocab/src/wire/mod.rs",
        &[
            "MAX_STATIC_STRINGS",
            "MAX_STATIC_STRING_BYTES",
            "RESERVED_IDS",
        ],
    ),
    (
        "crates/sunrise-mod-vocab/src/content/config_files.rs",
        &["ConfigFiles"],
    ),
];

/// The protocol number the pinned text belongs to, and the hash of that text.
const PINNED: (u32, &str) = (1, "362cc2f287f3e3279241e2643e6d24ca");

fn find(items: &[Item], name: &str, out: &mut Vec<String>) {
    for item in items {
        let ident = match item {
            Item::Const(i) => Some(i.ident.to_string()),
            Item::Static(i) => Some(i.ident.to_string()),
            Item::Fn(i) => Some(i.sig.ident.to_string()),
            Item::Struct(i) => Some(i.ident.to_string()),
            Item::Enum(i) => Some(i.ident.to_string()),
            Item::Trait(i) => Some(i.ident.to_string()),
            _ => None,
        };
        if ident.as_deref() == Some(name) {
            out.push(item.to_token_stream().to_string());
        }
        match item {
            Item::Impl(i) => {
                for member in &i.items {
                    match member {
                        syn::ImplItem::Fn(f) if f.sig.ident == name => {
                            out.push(f.to_token_stream().to_string())
                        }
                        syn::ImplItem::Const(c) if c.ident == name => {
                            out.push(c.to_token_stream().to_string())
                        }
                        _ => {}
                    }
                }
            }
            Item::Mod(m) => {
                if let Some((_, inner)) = &m.content {
                    find(inner, name, out);
                }
            }
            _ => {}
        }
    }
}

/// The text of every pinned item, in the table's order.
fn text(root: &Path) -> String {
    let mut text = String::new();
    for (file, names) in ITEMS {
        // Documentation is no part of the protocol.
        let source: String = std::fs::read_to_string(root.join(file))
            .unwrap()
            .lines()
            .filter(|line| !line.trim_start().starts_with("//"))
            .map(|line| format!("{line}\n"))
            .collect();
        let parsed = syn::parse_file(&source).unwrap();
        for name in *names {
            let mut found = Vec::new();
            find(&parsed.items, name, &mut found);
            assert!(!found.is_empty(), "{file}: no item {name}: update ITEMS");
            for item in found {
                text.push_str(&item);
                text.push('\n');
            }
        }
    }
    text
}

#[test]
#[ignore = "daily"]
fn the_shared_protocols_items_are_pinned_to_its_number() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let hash = mod_surface::shape::digest(&text(&root));
    assert_eq!(
        mod_surface::call::SHARED_PROTOCOL,
        PINNED.0,
        "update PINNED beside SHARED_PROTOCOL"
    );
    assert_eq!(
        hash, PINNED.1,
        "an item that defines the shared protocol changed (now {hash}): bump SHARED_PROTOCOL in sunrise-mod-surface/src/call.rs and update PINNED"
    );
}
