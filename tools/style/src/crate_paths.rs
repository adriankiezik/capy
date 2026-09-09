use crate::error::Result;
use crate::source::{Edit, Source, apply};
use proc_macro2::Span;
use syn::{
    spanned::Spanned,
    visit::{self, Visit},
};

struct Shadowing<'a> {
    name: &'a str,
    found: bool,
}

impl<'ast> Visit<'ast> for Shadowing<'_> {
    fn visit_item_extern_crate(&mut self, item: &'ast syn::ItemExternCrate) {
        let binding = item.rename.as_ref().map_or(&item.ident, |(_, name)| name);

        self.found |= item.ident != "self" && binding == self.name;
    }

    fn visit_item_union(&mut self, item: &'ast syn::ItemUnion) {
        self.found |= item.ident == self.name;

        visit::visit_item_union(self, item);
    }

    fn visit_item_trait(&mut self, item: &'ast syn::ItemTrait) {
        self.found |= item.ident == self.name;

        visit::visit_item_trait(self, item);
    }

    fn visit_item_mod(&mut self, item: &'ast syn::ItemMod) {
        self.found |= item.ident == self.name;

        visit::visit_item_mod(self, item);
    }

    fn visit_pat_ident(&mut self, pattern: &'ast syn::PatIdent) {
        self.found |= pattern.ident == self.name;

        visit::visit_pat_ident(self, pattern);
    }

    fn visit_type_param(&mut self, parameter: &'ast syn::TypeParam) {
        self.found |= parameter.ident == self.name;

        visit::visit_type_param(self, parameter);
    }

    fn visit_use_rename(&mut self, rename: &'ast syn::UseRename) {
        self.found |= rename.rename == self.name;
    }

    fn visit_use_path(&mut self, path: &'ast syn::UsePath) {
        self.found |= path.ident == self.name && imports_self(&path.tree);

        visit::visit_use_path(self, path);
    }

    fn visit_use_glob(&mut self, _: &'ast syn::UseGlob) {
        self.found = true;
    }

    fn visit_use_name(&mut self, name: &'ast syn::UseName) {
        self.found |= name.ident == self.name;
    }

    fn visit_item_type(&mut self, item: &'ast syn::ItemType) {
        self.found |= item.ident == self.name;

        visit::visit_item_type(self, item);
    }

    fn visit_item_struct(&mut self, item: &'ast syn::ItemStruct) {
        self.found |= item.ident == self.name;

        visit::visit_item_struct(self, item);
    }

    fn visit_item_enum(&mut self, item: &'ast syn::ItemEnum) {
        self.found |= item.ident == self.name;

        visit::visit_item_enum(self, item);
    }
}

fn imports_self(tree: &syn::UseTree) -> bool {
    match tree {
        syn::UseTree::Name(name) => name.ident == "self",
        syn::UseTree::Group(group) => group.items.iter().any(imports_self),
        _ => false,
    }
}

struct Paths<'a> {
    name: &'a str,
    shadowed: bool,
    prefixes: Vec<(Option<Span>, Span)>,
}

impl Paths<'_> {
    fn import(&mut self, tree: &syn::UseTree, leading: Option<Span>) {
        match tree {
            syn::UseTree::Path(path) if path.ident == self.name => {
                self.prefixes.push((leading, path.ident.span()))
            }
            syn::UseTree::Name(name) if name.ident == self.name => {
                self.prefixes.push((leading, name.ident.span()))
            }
            syn::UseTree::Rename(rename) if rename.ident == self.name => {
                self.prefixes.push((leading, rename.ident.span()))
            }
            syn::UseTree::Group(group) if leading.is_none() => {
                for tree in &group.items {
                    self.import(tree, None);
                }
            }
            _ => {}
        }
    }
}

impl<'ast> Visit<'ast> for Paths<'_> {
    fn visit_path(&mut self, path: &'ast syn::Path) {
        if let Some(first) = path.segments.first()
            && first.ident == self.name
            && path.segments.len() > 1
            && (!self.shadowed || path.leading_colon.is_some())
        {
            self.prefixes.push((
                path.leading_colon.as_ref().map(Spanned::span),
                first.ident.span(),
            ));
        }

        visit::visit_path(self, path);
    }

    fn visit_item_use(&mut self, item: &'ast syn::ItemUse) {
        if !self.shadowed || item.leading_colon.is_some() {
            self.import(&item.tree, item.leading_colon.as_ref().map(Spanned::span));
        }
    }
}

pub fn replace(text: &str, name: Option<&str>) -> Result<String> {
    let Some(name) = name else {
        return Ok(text.to_owned());
    };

    let file = syn::parse_file(text)?;

    let mut shadowing = Shadowing { name, found: false };

    shadowing.visit_file(&file);

    let mut paths = Paths {
        name,
        shadowed: shadowing.found,
        prefixes: Vec::new(),
    };

    paths.visit_file(&file);

    let source = Source::new(text);

    let edits = paths
        .prefixes
        .into_iter()
        .map(|(leading, name)| {
            let mut range = source.range(name)?;

            if let Some(leading) = leading {
                range.start = source.range(leading)?.start;
            }

            Ok(Edit {
                range,
                replacement: "crate".to_owned(),
            })
        })
        .collect::<Result<Vec<_>>>()?;

    apply(text, edits)
}
