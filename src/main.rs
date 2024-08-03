use std::ffi::CStr;
use std::fs;
use std::io::prelude::*;
use std::io::BufReader;
use std::path::{Path, PathBuf};

use anyhow::Context;
use clap::Parser;
use flate2::read::ZlibDecoder;
use flate2::write::ZlibEncoder;
use flate2::Compression;
use sha1::{Digest, Sha1};

/// Simple program to greet a person
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    #[command(subcommand)]
    command: Command,
}

// Enum to store the supported commands
#[derive(Parser, Debug)]
enum Command {
    Init,
    CatFile {
        #[clap(short = 'p')]
        pretty_print: bool,
        object_hash: String
    },
    HashObject {
        #[clap(short = 'w')]
        write: bool,
        file: PathBuf,
    }
}

enum Kind {
    Blob
}

fn main() -> anyhow::Result<()>{
    let args = Args::parse();

    match args.command {
        Command::Init => {
            fs::create_dir(".git").unwrap();
            // Subfolder for git objects; blobs, trees, commits and tags
            fs::create_dir(".git/objects").unwrap();
            // Subfolder for references to the .git/objects objetcs
            fs::create_dir(".git/refs").unwrap();
            // Ref to the current branch
            fs::write(".git/HEAD", "ref: refs/heads/main\n").unwrap();
            println!("Git directory initialized")
        }
        Command::CatFile { pretty_print, object_hash } => {
            anyhow::ensure!( pretty_print, "mode must be given without -p, and we don't support mode" );
            
            // Open the hash's object file
            let f = std::fs::File::open(format!(
                ".git/objects/{}/{}",
                &object_hash[..2],  // Reference to the first 2 characters of object_hash
                &object_hash[2..]   // Reference to the rest of the characters of object_hash but the first 2
            )).context("open in .git/objects")?;

            // Create a zlib decoder for the file
            let z = ZlibDecoder::new(f);

            // Wrap the decoder in a buffer so the reading is buffered instead of streamed, increasing performance
            let mut z = BufReader::new(z);

            let mut buf = Vec::new();
            z.read_until(0, &mut buf).context("reading header from .git/objects")?;

            // Read  the header until the null character creating a C string from ther buffer
            let header = CStr::from_bytes_with_nul(&buf).expect("No nul character to delimit the header");

            // Convert the header back to str
            let header = header.to_str().context(".git/objects file header isn't valid UTF-8")?;

            // Parsse the git object type and size in B from the header
            let Some((kind, size)) = header.split_once(' ') else {
                anyhow::bail!(
                    ".git/objects wrong header format '{header}'"
                );
            };

            // Check and save the object type
            let kind = match kind {
                "blob" => Kind::Blob,
                _ => anyhow::bail!("Unsupported object type '{kind}'"),
            };

            // Check and save the object size
            let size = size.parse::<usize>().context(".git/objects file header has invalid size: {size}")?;

            buf.clear();
            buf.resize(size, 0);
            z.read_exact(&mut buf[..]).context("reading contents of .git/objects file")?;

            // Read one more byte from the buffer to make sure theres only an EOF left
            let n = z.read(&mut [0]).context("validating EOF in .git/object file")?;
            anyhow::ensure!(n == 0, ".git/object file had {n} trailing bytes");

            // Pritn object file contents
            let stdout = std::io::stdout();
            let mut stdout = stdout.lock();

            match kind {
                Kind::Blob => stdout
                    .write_all(&buf)
                    .context("writing object contents to stdout")?,
            }

        },
        Command::HashObject { write, file } => {
            fn write_blob<W>(file: &Path, writer: W) -> anyhow::Result<String>
            where
                W: Write,
            {
                let stat = std::fs::metadata(&file).with_context(|| format!("stat {}", file.display()))?;
                let writer = ZlibEncoder::new(writer, Compression::default());
                let mut writer = HashWriter { writer, hasher: Sha1::new()};
                // Write the object type
                write!(writer, "blob ")?;

                // Write the filel length and the nul character
                write!(writer, "{}\0", stat.len())?;

                // Open the file and copy it's content using the writer to compress it
                let mut file = std::fs::File::open(&file).with_context(|| format!("open {}", file.display()))?;
                std::io::copy(&mut file, &mut writer).context("stream file into blob")?;
                let _ = writer.writer.finish()?;

                // Retrieve the hash from the hasher
                let hash = writer.hasher.finalize();

                // Return Ok with the hash
                Ok(hex::encode(hash))
            }

            let hash = if write {
                // Write blob to temporary file
                let tmp = "temporary";
                let hash = write_blob(
                    &file,
                    std::fs::File::create(tmp).context("construct temporary file for blob")?,
                ).context("write out blob object")?;

                // Move temporary file to correct subfolder and file
                fs::create_dir_all(format!(".git/objects/{}/", &hash[..2]))
                    .context("create subdir of .git/objects")?;
                std::fs::rename(tmp, format!(".git/objects/{}/{}", &hash[..2], &hash[2..]))
                    .context("move blob file into .git/objects")?;

                // Return the hash
                hash
            } else {
                write_blob(&file, std::io::sink()).context("write out blob object")?
            };
            println!("{hash}");
        }
        
    }

    Ok(())
}

struct HashWriter<W> {
    writer: W,
    hasher: Sha1,
}
impl<W> Write for HashWriter<W> where W: Write {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let n = self.writer.write(buf)?;
        self.hasher.update(&buf[..n]);
        Ok(n)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.writer.flush()
    }
}
