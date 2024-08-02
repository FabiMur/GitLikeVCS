use std::env;
use std::fs;

fn main() {
    // Create .git folder
    let args: Vec<String> = env::args().collect();
    if args[1] == "init" {
        fs::create_dir(".git").unwrap();
        // Subfolder for git objects; blobs, trees, commits and tags
        fs::create_dir(".git/objects").unwrap();
        // Subfolder for references to the .git/objects objetcs
        fs::create_dir(".git/refs").unwrap();
        // Ref to the current branch
        fs::write(".git/HEAD", "ref: refs/heads/main\n").unwrap();
        println!("Git directory initialized")
    } else {
        println!("command not supported: {}", args[1])
    }
}
