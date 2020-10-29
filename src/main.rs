use std::io::prelude::*;

pub mod models;
use models::*;

fn help() {
    eprintln!(
        "OVERVIEW: assuo patch maker

USAGE: assuo [--url source_url]/[--file file_location]/[--init]/[--help]

OPTIONS:
--url    Loads an assuo patch file from the internet.
--file   Loads an assuo patch file from disk.
--init   Makes a new blank assuo patch file.
--help   Prints help.
"
    );
}

fn init(file_name: Option<String>) {
    let assuo_config = r#"[source]
    text = "Hello!"
    
    [[patch]]
    do = "insert"
    way = "post"
    spot = 4
    source = { text = ", World" }
    "#;

    if let Some(file_name) = file_name {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(file_name)
            .expect("couldn't open file for writing");

        // TODO: when https://github.com/alexcrichton/toml-rs/issues/265 gets resolved, this should
        // get looked at so there's compile time validation of the data we're writing to disk
        file.write_all(assuo_config.as_bytes())
            .expect("couldn't write to file");
    } else {
        println!("{}", assuo_config);
    }
}

fn do_patch(file: AssuoFile) -> Vec<u8> {
    // in the future, it would be nice to be able to apply patches as they come along so that everything is
    // non-blocking and fast, but for now, it's much simpler to "resolve everything -> apply patches"

    // resolve the base
    let mut file = file.resolve();

    // resolve every patch
    let patches = file
        .patch
        .unwrap_or_default()
        .into_iter()
        .map(|p| p.resolve())
        .collect::<Vec<_>>();

    // so right now i'm just going for simplicity rather than speed, so i just need a method that works for these patches
    // one ideal thing to do is to maintain another Vec with a Vec of indexes that is in the original file
    // really bad in terms of performance, *but* it is simple for finding the index something should be at

    let mut indexes = Vec::with_capacity(file.source.len());
    for i in 0..file.source.len() {
        indexes.push(vec![i]);
    }

    fn get_index(indexes: &Vec<Vec<usize>>, i: usize) -> usize {
        for (idx, index) in indexes.iter().enumerate() {
            if index.contains(&i) {
                return idx;
            }
        }

        panic!("assuo patch out of bounds?");
    }

    // now, we apply each patch sequentially, maintaining the indexes vec as we go
    for patch in patches {
        match patch {
            AssuoPatch::Insert { way, spot, source } => {
                let insertion_point = get_index(&indexes, spot);

                let insertion_point = match way {
                    Direction::Pre => insertion_point,
                    Direction::Post => insertion_point + 1,
                };

                indexes.splice(
                    insertion_point..insertion_point,
                    (0..source.len()).map(|_| vec![std::usize::MAX]),
                );

                file.source.splice(insertion_point..insertion_point, source);
            }
            AssuoPatch::Remove { way, spot, count } => {
                let insertion_point = get_index(&indexes, spot);

                let insertion_point = match way {
                    Direction::Post => insertion_point + 1,
                    Direction::Pre => insertion_point - count,
                };

                let fold = indexes[insertion_point..(insertion_point + count)]
                    .iter()
                    .fold(Vec::new(), |mut acc, elem| {
                        for element in elem {
                            if !acc.contains(element) {
                                acc.push(*element);
                            }
                        }
                        acc
                    });

                indexes.splice(insertion_point..(insertion_point + count), vec![fold]);

                file.source
                    .splice(insertion_point..(insertion_point + count), vec![]);
            }
        }
    }

    file.source
}

#[paw::main]
fn main(args: paw::Args) {
    // ARGUMENT PARSING:
    // assuo aims to "do one thing, and do it right". our arg parsing aims to capture the unix philosophy by giving a
    // similar experience to what tools like `cat` offer.
    //
    // SUPPORTED SCENARIOS:
    //
    //     print out the help
    // assuo
    // assuo --help
    // assuo -h
    // assuo /help
    // assuo /h
    // assuo -?
    // assuo /?
    //
    //     initialize a blank assuo file named `assuo.toml`
    // assuo --init
    // assuo -i
    // assuo /init
    // assuo /i
    //
    //     initialize a blank assuo file named `x`
    // assuo --init x
    // assuo -i x
    // assuo /init x
    // assuo /i x
    //
    //     run patches for an assuo file named `assuo.toml`
    // assuo
    // cat assuo.toml | assuo
    //
    //     run patches for an assuo file named `x`
    // assuo x
    // assuo --file x
    // assuo -f x
    // assuo /file x
    // assuo /f x
    // cat x | assuo
    //
    //     run patches for an assuo file located at the URL `https://x`
    // assuo --url https://x
    // assuo -u https://x
    // assuo /url https://x
    // assuo /u https://x
    // wget -O - https://x | assuo

    // TODO: clean up mess

    let mut got_arg = false;
    let mut do_init = false;

    for arg in args.skip(1) {
        got_arg = true;
        if do_init {
            init(Some(arg));
            return;
        }

        let trim_for_arg = if arg.starts_with("--") {
            2
        } else if arg.starts_with("-") {
            1
        } else if arg.starts_with("/") {
            1
        } else {
            0
        };

        if trim_for_arg > 0 {
            let arg = &arg[trim_for_arg..];

            if arg == "?" || arg == "h" || arg == "help" {
                help();
                return;
            } else if arg == "i" || arg == "init" {
                do_init = true;
            }
        } else {
            let config =
                toml::from_str::<AssuoFile>(&std::fs::read_to_string(arg).unwrap()).unwrap();
            do_patch(config);
            return;
        }
    }

    if do_init {
        init(None);
        return;
    }

    // if we didn't get anything, try to read from an assuo.toml file to print that out
    let assuo_config = match std::fs::read_to_string("assuo.toml") {
        Ok(assuo_config) => assuo_config,
        Err(_) => {
            // consume from stdin if the file doesn't exist
            let mut buffer = Vec::new();
            std::io::stdin().lock().read_to_end(&mut buffer).unwrap();
            String::from_utf8(buffer).unwrap()
        }
    };

    // TODO: display help if no "assuo.toml" found (and print that no assuo.toml was found, showing help)
    let config = toml::from_str::<AssuoFile>(&assuo_config).unwrap();
    let patch = do_patch(config);
    std::io::stdout()
        .lock()
        .write_all(&patch)
        .expect("to print to stdout");
}
