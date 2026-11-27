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
        // `.gitkeep`, `.DS_Store` and the like are not part of a pack.
        if name.starts_with('.') {
            continue;
        }
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

#[cfg(test)]
mod review_tests {
    #[cfg(unix)]
    use std::os::unix::{ffi::OsStrExt, fs::symlink};

    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/scratch/review-165/macro")
            .join(name);
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn r_an_empty_folder_gives_no_files() {
        let dir = scratch("empty");
        assert!(collect_files(&dir).unwrap().is_empty());
    }

    #[test]
    fn r_order_is_sorted_depth_first_whatever_the_creation_order() {
        let dir = scratch("order");
        for path in ["z.json", "data/b/x.json", "data/a/y.json", "data/a.json"] {
            let target = dir.join(path);
            fs::create_dir_all(target.parent().unwrap()).unwrap();
            fs::write(target, [0xff, 0x00, 0xfe]).unwrap();
        }
        let relative: Vec<String> = collect_files(&dir)
            .unwrap()
            .into_iter()
            .map(|(relative, _)| relative)
            .collect();
        assert_eq!(
            relative,
            ["data/a/y.json", "data/a.json", "data/b/x.json", "z.json"]
        );
    }

    #[test]
    fn r_names_that_start_with_a_dot_are_skipped() {
        let dir = scratch("dot");
        for path in [".hidden", "data/.gitkeep", ".git/config", "data/x.json"] {
            let target = dir.join(path);
            fs::create_dir_all(target.parent().unwrap()).unwrap();
            fs::write(target, b"{}").unwrap();
        }
        let relative: Vec<String> = collect_files(&dir)
            .unwrap()
            .into_iter()
            .map(|(relative, _)| relative)
            .collect();
        assert_eq!(relative, ["data/x.json"]);
    }

    // APFS refuses a file name that is not UTF-8, so macOS cannot create the fixture.
    #[cfg(all(unix, not(target_os = "macos")))]
    #[test]
    fn r_a_non_utf8_file_name_is_an_error() {
        let dir = scratch("non_utf8");
        fs::write(
            dir.join(std::ffi::OsStr::from_bytes(b"bad\xff.json")),
            b"{}",
        )
        .unwrap();
        let error = collect_files(&dir).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    }

    #[cfg(unix)]
    #[test]
    fn r_a_symlink_loop_is_an_error_not_a_hang() {
        let dir = scratch("loop");
        symlink(".", dir.join("self")).unwrap();
        assert!(collect_files(&dir).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn r_a_dangling_symlink_is_an_error() {
        let dir = scratch("dangling");
        symlink("missing.json", dir.join("link.json")).unwrap();
        assert!(collect_files(&dir).is_err());
    }
}
