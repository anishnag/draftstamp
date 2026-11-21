//! `.draftstampignore`: a small, gitignore-like list of files and folders to leave out of a stamp.
//!
//! One pattern per line; blank lines and lines starting with `#` are ignored. `*` matches any run
//! of characters and `?` one character, never crossing `/`. A pattern without `/` matches a name
//! anywhere (`*.aux`, `build`); a pattern with `/` matches from the stamped folder's root
//! (`figures/raw/*`). A trailing `/` matches folders only (`build/`).

pub(crate) const IGNORE_FILE: &str = ".draftstampignore";

#[derive(Debug, Default)]
pub(crate) struct Ignore {
    patterns: Vec<Pattern>,
}

#[derive(Debug)]
struct Pattern {
    glob: String,
    anchored: bool,
    dir_only: bool,
}

impl Ignore {
    pub(crate) fn parse(text: &str) -> Ignore {
        let patterns = text
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty() && !l.starts_with('#'))
            .map(|line| {
                let dir_only = line.ends_with('/');
                let line = line.trim_end_matches('/').trim_start_matches('/');
                Pattern {
                    glob: line.to_owned(),
                    anchored: line.contains('/'),
                    dir_only,
                }
            })
            .filter(|p| !p.glob.is_empty())
            .collect();
        Ignore { patterns }
    }

    /// Whether `rel` (relative to the stamped folder, `/`-separated) should be left out.
    pub(crate) fn matches(&self, rel: &str, is_dir: bool) -> bool {
        let name = rel.rsplit('/').next().unwrap_or(rel);
        self.patterns
            .iter()
            .any(|p| (!p.dir_only || is_dir) && glob(&p.glob, if p.anchored { rel } else { name }))
    }
}

/// `*` and `?` wildcards that never match `/`.
fn glob(pattern: &str, text: &str) -> bool {
    let (p, t): (Vec<char>, Vec<char>) = (pattern.chars().collect(), text.chars().collect());
    let (mut pi, mut ti) = (0, 0);
    let mut star: Option<(usize, usize)> = None;
    while ti < t.len() {
        if pi < p.len() && (p[pi] == '?' && t[ti] != '/' || p[pi] == t[ti] && p[pi] != '*') {
            pi += 1;
            ti += 1;
        } else if pi < p.len() && p[pi] == '*' {
            star = Some((pi, ti));
            pi += 1;
        } else if let Some((sp, st)) = star
            && t[st] != '/'
        {
            pi = sp + 1;
            ti = st + 1;
            star = Some((sp, st + 1));
        } else {
            return false;
        }
    }
    p[pi..].iter().all(|c| *c == '*')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn globs() {
        assert!(glob("*.aux", "main.aux"));
        assert!(!glob("*.aux", "main.tex"));
        assert!(glob("ch?.tex", "ch3.tex"));
        assert!(glob("*", "anything"));
        assert!(
            !glob("figures/*", "figures/raw/a.png"),
            "* does not cross /"
        );
        assert!(glob("figures/*/a.png", "figures/raw/a.png"));
    }

    #[test]
    fn patterns() {
        let ig = Ignore::parse("# LaTeX outputs\n*.aux\n\nbuild/\nfigures/raw/*\n/notes.txt\n");
        assert!(ig.matches("main.aux", false));
        assert!(
            ig.matches("chapters/one.aux", false),
            "unanchored matches anywhere"
        );
        assert!(ig.matches("build", true));
        assert!(!ig.matches("build", false), "trailing / means folders only");
        assert!(ig.matches("figures/raw/big.png", false));
        assert!(
            !ig.matches("other/figures/raw/big.png", false),
            "anchored to the root"
        );
        assert!(ig.matches("notes.txt", false));
        assert!(!ig.matches("main.tex", false));
        assert!(!Ignore::parse("").matches("main.tex", false));
    }
}
