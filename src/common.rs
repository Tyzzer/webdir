use std::path::{ Path, PathBuf, Component };


pub fn path_canonicalize<P: AsRef<Path>>(root: &Path, path: P) -> (usize, PathBuf) {
    path.as_ref()
        .components()
        .fold((0, root.to_path_buf()), |(mut depth, mut sum), next| {
            match next {
                Component::Normal(p) => {
                    sum.push(p);
                    depth += 1;
                },
                Component::ParentDir if depth > 0 => if sum.pop() {
                    depth -= 1;
                },
                _ => ()
            };
            (depth, sum)
        })
}
