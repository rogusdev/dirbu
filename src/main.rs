use std::fs::{read_dir, set_permissions, DirBuilder, DirEntry, File, Permissions, ReadDir};
use std::iter::Flatten;
use std::path::{Path, PathBuf};
// use std::time::SystemTime;
use std::io::{BufRead, BufReader, Lines, Result as IoResult}; // BufRead for lines
use std::os::unix::fs::DirBuilderExt;
use std::os::unix::fs::PermissionsExt;

// #[derive(Debug)]
// struct FileTimes {
//     created: SystemTime,
//     modified: SystemTime,
//     accessed: SystemTime,
// }

// impl TryFrom<Metadata> for FileTimes {
//     type Error = std::io::Error;

//     fn try_from(value: Metadata) -> Result<Self, Self::Error> {
//         Ok(Self {
//             created: value.created()?,
//             modified: value.modified()?,
//             accessed: value.accessed()?,
//         })
//     }
// }

type Mode = u32;
type Size = u64;

#[derive(Debug, Clone)]
enum Entry {
    Unknown(String),
    Dir(String, Mode, Vec<Entry>), //, FileTimes
    File(String, Mode, Size),      //, FileTimes
}

impl Entry {
    fn is_dir(&self) -> bool {
        match self {
            Self::Dir(_, _, _) => true,
            _ => false,
        }
    }

    fn filename(&self) -> String {
        match self {
            Self::Dir(filename, _, _) => filename.clone(),
            Self::File(filename, _, _) => filename.clone(),
            Self::Unknown(filename) => filename.clone(),
        }
    }

    fn print(&self, depth: usize, path: &Path) {
        let indent = "\t".repeat(depth);
        let parent = "";//path.display().to_string();
        match self {
            Self::Dir(filename, mode, entries) => {
                println!("{indent}DIR\t{parent}{filename}\t{mode}");
                for entry in entries {
                    entry.print(depth + 1, &path.join(filename));
                }
            }
            Self::File(filename, mode, len) => {
                println!("{indent}FIL\t{parent}{filename}\t{mode}\t{len}");
            }
            Self::Unknown(filename) => {
                println!("{indent}UNK\t{parent}{filename}");
            }
        }
    }

    fn from(value: DirEntry, count: &mut usize, depth: usize) -> Self {
        *count += 1;
        eprint!("\rEntries: {count}");

        match value.file_name().into_string() {
            Ok(filename) => match value.metadata() {
                Ok(metadata) => {
                    let mode = metadata.permissions().mode();
                    // match FileTimes::try_from(metadata.clone()) {
                    //     Ok(times) => {
                    if metadata.is_dir() {
                        let entries = if depth < 20 {
                            match read_dir(&value.path()) {
                                Ok(read_from) => get_files(read_from, count, depth + 1),
                                Err(e) => vec![Entry::Unknown(format!(
                                    "Error collecting dir entries? {e:?}"
                                ))],
                            }
                        } else {
                            println!("DIR {} is too DEEP {depth}!", value.path().display());
                            vec![]
                        };
                        Entry::Dir(filename, mode, entries) //, times
                    } else if metadata.is_file() {
                        Entry::File(filename, mode, metadata.len()) //, times
                    } else {
                        Entry::Unknown(format!("Symlink?: {filename}"))
                    }
                    //     }
                    //     Err(_) => Entry::Unknown(format!("Unrequestable file times: {filename}")),
                    // }
                }
                Err(_) => Entry::Unknown(format!("Unrequestable metadata: {filename}")),
            },
            Err(oss) => Entry::Unknown(format!("Unparseable filename: {oss:?}")),
        }
    }

    fn copy(&self, ancestors: &Vec<String>, dst_str: &String) {
        use std::fs::copy;
        use std::iter::once;

        let src_root_len = ancestors.get(0).unwrap().len() + 1;
        let dst_root_len = dst_str.len();

        match self {
            Self::Dir(_, mode, _) => {
                let mode = *mode;

                // no need for filename, dir name is already in ancestors
                let path_src: PathBuf = ancestors.iter().collect();
                let path_dst: PathBuf = once(dst_str).chain(ancestors.iter().skip(1)).collect();

                println!("DIR: {} -> {}", path_src.display(), path_dst.display());
                assert_eq!(
                    path_src.display().to_string()[src_root_len..],
                    path_dst.display().to_string()[dst_root_len..]
                );

                if path_dst.exists() {
                    match path_dst.metadata() {
                        Ok(metadata) => {
                            if metadata.permissions().mode() != mode {
                                if let Err(e) =
                                    set_permissions(&path_dst, Permissions::from_mode(mode))
                                {
                                    println!(
                                        "FAILED setting permissions on directory!\n{}\n{e:?}",
                                        path_dst.display()
                                    );
                                }
                            }
                        }
                        Err(e) => println!(
                            "FAILED getting metadata from directory!\n{}\n{e:?}",
                            path_dst.display()
                        ),
                    }
                } else {
                    // https://github.com/rust-lang/rust/issues/22415#issuecomment-1284783730
                    if let Err(e) = DirBuilder::new().mode(mode).create(&path_dst) {
                        println!(
                            "FAILED creating directory with mode!\n{}\n{e:?}",
                            path_dst.display()
                        );
                    }
                }
            }

            Self::File(_, mode, len) => {
                let mode = *mode;
                let len = *len;

                let filename = self.filename();
                let path_src: PathBuf = ancestors.iter().chain(once(&filename)).collect();
                let path_dst: PathBuf = once(dst_str)
                    .chain(ancestors.iter().skip(1).chain(once(&filename)))
                    .collect();

                println!("FIL: {} -> {}", path_src.display(), path_dst.display());
                assert_eq!(
                    path_src.display().to_string()[src_root_len..],
                    path_dst.display().to_string()[dst_root_len..]
                );

                if path_dst.exists() {
                    match path_dst.metadata() {
                        Ok(metadata) => {
                            if metadata.len() != len {
                                if let Err(e) = copy(&path_src, &path_dst) {
                                    println!(
                                        "FAILED updating file!\n{}\n{}\n{e:?}",
                                        path_src.display(),
                                        path_dst.display()
                                    );
                                }
                            } else if metadata.permissions().mode() != mode {
                                if let Err(e) =
                                    set_permissions(&path_dst, Permissions::from_mode(mode))
                                {
                                    println!(
                                        "FAILED setting permissions on file!\n{}\n{}\n{e:?}",
                                        path_src.display(),
                                        path_dst.display()
                                    );
                                }
                            }
                        }
                        Err(e) => println!(
                            "FAILED getting metadata from file!\n{}\n{}\n{e:?}",
                            path_src.display(),
                            path_dst.display()
                        ),
                    }
                } else {
                    if let Err(e) = copy(&path_src, &path_dst) {
                        println!(
                            "FAILED creating file!\n{}\n{}\n{e:?}",
                            path_src.display(),
                            path_dst.display()
                        );
                    }
                }
            }
            Self::Unknown(_) => {}
        }
    }
}

fn main() {
    let mut args = std::env::args().skip(1);

    // get the src path and optional dst path
    if let Ok((path_src, path_dst)) = process_args(args.next(), args.next()) {
        match path_dst {
            Some(path_dst) => {
                // files get read as entries output, to then copy to dst
                read_entries_and_copy(path_src, path_dst);
            }
            None => {
                // dirs get read to build entries output
                read_and_output_tree(path_src);
            }
        }
    } else {
        println!("Must provide a dir path for SRC (FROM) and no DST (TO) when collecting files!");
        println!("Must provide a file path for SRC (FROM) and a dir path for DST (TO) when copying files!");
    }
}

fn process_args(
    arg_src: Option<String>,
    arg_dst: Option<String>,
) -> Result<(PathBuf, Option<PathBuf>), ()> {
    match arg_dst {
        // if dst is present, src is a file
        Some(arg_dst) => match arg_src {
            Some(arg_src) => {
                let path_src = PathBuf::from(&arg_src);
                if path_src.is_file() {
                    let path_dst = PathBuf::from(&arg_dst);
                    if path_dst.is_dir() {
                        Ok((path_src, Some(path_dst)))
                    } else {
                        Err(())
                    }
                } else {
                    Err(())
                }
            }
            None => Err(()),
        },
        // if dst is not present, src is a dir
        None => match arg_src {
            Some(arg_src) => {
                let path_src = PathBuf::from(&arg_src);
                if path_src.is_dir() {
                    Ok((path_src, None))
                } else {
                    Err(())
                }
            }
            None => Err(()),
        },
    }
}

fn read_and_output_tree(src_dir: PathBuf) {
    let count = &mut 0;
    match read_dir(&src_dir) {
        Ok(read_src) => {
            let entries = get_files(read_src, count, 0);
            let src_str = src_dir.display().to_string();
            let dir_entry = Entry::Dir(src_str, 0, entries.clone());
            dir_entry.print(0, &src_dir);
        }
        Err(e) => println!("SRC (FROM) is not a dir when it was?... {e:?}"),
    }
}

fn read_entries_and_copy(src_file: PathBuf, dst_dir: PathBuf) {
    if let Ok(lines) = read_lines(src_file) {
        let mut depth = 0;
        let mut was_dir = false;
        let dst_str = dst_dir.display().to_string();
        let mut ancestors = vec![];
        let mut count = 0;

        for line in lines {
            // https://users.rust-lang.org/t/fastest-way-to-count-number-of-characters-beginning-a-string/75421
            let indent: usize = line
                .chars()
                .take_while(|ch| *ch == '\t')
                // https://doc.rust-lang.org/std/iter/trait.Iterator.html#method.fold
                .fold(0, |acc, _| acc + 1);

            let entry = if indent == depth {
                parse_line(indent, line).expect("BAD input!")
            } else if indent == depth + 1 {
                if !was_dir {
                    println!("Indent INCREASED when not following dir! Quitting at:\n{line}");
                    return;
                }
                parse_line(indent, line).expect("BAD input!")
            } else if indent < depth {
                // diff between indent and depth is how much to pop
                for _ in 0..(depth - indent) {
                    ancestors.pop();
                }
                parse_line(indent, line).expect("BAD input!")
            } else {
                println!("Indent moved TOO FAR from depth! Quitting at:\n{line}");
                return;
            };

            if entry.is_dir() {
                ancestors.push(entry.filename());
                was_dir = true;
            } else {
                was_dir = false;
            }
            depth = indent;

            if entry.is_dir() {
                // skip initial src folder entry
                if ancestors
                    .iter()
                    .next()
                    .cloned()
                    .expect("first ancestor always exists")
                    == entry.filename()
                {
                    continue;
                }
            }

            count += 1;
            eprint!("\rEntries: {count}");
            entry.copy(&ancestors, &dst_str);
        }
    }
}

fn get_files(read_src: ReadDir, count: &mut usize, depth: usize) -> Vec<Entry> {
    let mut entries = Vec::new();
    for entry in read_src {
        match entry {
            Ok(entry) => entries.push(Entry::from(entry, count, depth)),
            Err(e) => println!("Error in dir! {e:?}"),
        }
    }
    entries
}

// https://doc.rust-lang.org/rust-by-example/std_misc/file/read_lines.html
fn read_lines<P>(filename: P) -> IoResult<Flatten<Lines<BufReader<File>>>>
where
    P: AsRef<Path>,
{
    let file = File::open(filename)?;
    Ok(BufReader::new(file).lines().flatten())
}

fn parse_line(indent: usize, line: String) -> Result<Entry, ()> {
    let mut cols = line[indent..].split("\t");
    match cols.next() {
        Some(id) => match id {
            "DIR" => {
                let filename = cols
                    .next()
                    .expect(&format!("MISSING filename: {line}"))
                    .to_owned();
                let mode = cols
                    .next()
                    .expect(&format!("MISSING mode: {line}"))
                    .parse::<u32>()
                    .expect(&format!("INVALID mode: {line}"));
                Ok(Entry::Dir(filename, mode, vec![]))
            }
            "FIL" => {
                let filename = cols
                    .next()
                    .expect(&format!("MISSING filename: {line}"))
                    .to_owned();
                let mode = cols
                    .next()
                    .expect(&format!("MISSING mode: {line}"))
                    .parse::<u32>()
                    .expect(&format!("INVALID mode: {line}"));
                let len = cols
                    .next()
                    .expect(&format!("MISSING len: {line}"))
                    .parse::<u64>()
                    .expect(&format!("INVALID len: {line}"));
                Ok(Entry::File(filename, mode, len))
            }
            "UKN" => {
                println!("Unexpected UNKNOWN:\n{line}");
                Err(())
            }
            _ => {
                println!("UNRECOGNIZED line:\n{line}");
                Err(())
            }
        },
        None => {
            println!("EMPTY line! Unparseable");
            Err(())
        }
    }
}
