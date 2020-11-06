use std::{
    env, fs::{File, read_dir, remove_file, rename}, io::{BufRead, BufReader, Write}, 
    path::{Path, PathBuf}
};

type Result<T> = std::result::Result<T, failure::Error>;

fn count_braces(str: &String) -> isize {
    let mut braces = 0;
    for char in str.chars() {
        match char {
            '{' => braces += 1,
            '}' => braces -= 1,
            _ => ()
        }   
    }
    braces
}

fn parse_file(path: &PathBuf) -> Result<()> {
    let src = File::open(path).map_err(
        |err| failure::err_msg(
            format!("{}, file {}", err, path.display().to_string())
        )
    )?;
    let mut tmp = File::create(&Path::new("tmp"))?;
    let mut braces = 0;
    let mut flags = 0;
    let mut skip = false;
    for line in BufReader::new(src).lines() { 
        let line = line?;
        let what = line.trim().to_lowercase();
        if what.starts_with("#[cfg(test)]") {
            braces = count_braces(&what);
            assert!(braces >= 0);
            skip = true;
        }
        if !skip {
            tmp.write_all(line.as_bytes())?;
            tmp.write_all("\n".as_bytes())?
        } else {
            let char_n = what.len();
            let char_0 = what.get(0..1);
            let char_1 = what.get(1..2);
            let char_n = what.get(char_n-1..char_n);
            if let Some(char_0) = char_0 {
                let char_1 = char_1.unwrap_or("");
                let char_n = char_n.ok_or_else(
                    || failure::err_msg("Internal error: cannot get last char in line") 
                )?;
                if (char_0 == "#") && (char_1 == "[") {
                    continue
                }
                if (char_0 == "/") && (char_1 == "/") {
                    continue
                }
                braces += count_braces(&what);
                assert!(braces >= 0);
                if (char_n != ";") && (char_n != ",") && (char_n != "}") {
                    continue
                }
            }
            skip = (braces > 0) || (flags != 0)
        }
    }
    Ok(())
}

fn process_file(path: &PathBuf) -> Result<()> {
    parse_file(path)?;
    remove_file(path)?;
    rename("tmp", path)?;
    Ok(())
}
     
fn process(path: &PathBuf) -> Result<()> {
    if path.is_dir() {
        println!("{}", path.display().to_string());
        for entry in read_dir(path)? {
            process(&entry?.path())?
        }
    } else if let Some(ext) = path.extension() {
        if let Some("rs") = ext.to_str() {
            println!("{}", path.display().to_string());
            process_file(path)?
        }
    }
    Ok(())
}

fn main() {
    let mut pathes = Vec::new();
    for arg in env::args().skip(1) {
        pathes.push(arg)
    }
    if pathes.is_empty() {
        println!("Usage: notests <path-to-sources> [<path-to-sources>..]");
    } else {
        for path in pathes {
            process(&PathBuf::from(path)).unwrap_or_else(
                |e| println!("Tests stripping error: {}", e)
            )
        }
    }
}
