//! `include_data_pack!`: embeds a folder of a mod crate as the files of a data pack.

use std::{
    fs, io,
    path::{Path, PathBuf},
};

use proc_macro::TokenStream;
use quote::quote;
use syn::{LitStr, parse_macro_input};

/// The files below `dir`, sorted, as `(path relative to dir with '/' separators, absolute path)`.
fn collect_files(dir: &Path) -> io::Result<Vec<(String, PathBuf)>> {
    let mut files = Vec::new();
    collect_into(dir, "", &mut files)?;
    Ok(files)
}

fn collect_into(dir: &Path, prefix: &str, files: &mut Vec<(String, PathBuf)>) -> io::Result<()> {
    let mut entries = fs::read_dir(dir)?.collect::<io::Result<Vec<_>>>()?;
    entries.sort_by_key(fs::DirEntry::file_name);
    for entry in entries {
        let name = entry.file_name().into_string().map_err(|name| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("the file name {} is not UTF-8", name.to_string_lossy()),
            )
        })?;
        let path = entry.path();
        if path.to_str().is_none() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("the path {} is not UTF-8", path.display()),
            ));
        }
        let relative = format!("{prefix}{name}");
        // `fs::metadata` follows symlinks, so a linked folder or file is embedded too.
        if fs::metadata(&path)?.is_dir() {
            collect_into(&path, &format!("{relative}/"), files)?;
        } else {
            files.push((relative, path));
        }
    }
    Ok(())
}

pub fn expand(input: TokenStream) -> TokenStream {
    let literal = parse_macro_input!(input as LitStr);
    let Some(manifest_dir) = std::env::var_os("CARGO_MANIFEST_DIR") else {
        return syn::Error::new(literal.span(), "CARGO_MANIFEST_DIR is not set")
            .to_compile_error()
            .into();
    };
    let root = Path::new(&manifest_dir).join(literal.value());
    let files = match collect_files(&root) {
        Ok(files) => files,
        Err(error) => {
            return syn::Error::new(
                literal.span(),
                format!(
                    "cannot read the data pack folder {}: {error}",
                    root.display()
                ),
            )
            .to_compile_error()
            .into();
        }
    };
    let entries = files.iter().map(|(relative, path)| {
        let absolute = path.to_str().unwrap_or_default();
        quote! { (#relative, include_bytes!(#absolute).as_slice()) }
    });
    // A `const` makes the slice `'static` wherever the macro is used.
    quote! {{
        const FILES: &[(&str, &[u8])] = &[#(#entries),*];
        FILES
    }}
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collects_nested_files_sorted_with_slash_separators() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data_pack");
        let files = collect_files(&root).unwrap();
        let relative: Vec<&str> = files
            .iter()
            .map(|(relative, _)| relative.as_str())
            .collect();
        assert_eq!(relative, ["data/gems/loot_table/ruby.json", "pack.mcmeta"]);
        assert!(files.iter().all(|(_, path)| path.is_file()));
    }

    #[test]
    fn a_missing_folder_is_an_error() {
        assert!(collect_files(Path::new("does/not/exist")).is_err());
    }
}
