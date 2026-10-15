use std::fmt;

use crate::block::Block;
use crate::style::paint;
use crate::style::palette::{LINE, ROSE, SOFT, TEXT};

/// A hierarchy drawn with thin guides. Branches are brighter than leaves.
///
/// ```no_run
/// use tinsel::Tree;
/// Tree::new("dot")
///     .child(Tree::new("tube").leaf("ios").leaf("macos"))
///     .leaf("README.md")
///     .print();
/// ```
#[derive(Clone, Debug)]
pub struct Tree {
    label: String,
    children: Vec<Tree>,
}

impl Tree {
    /// A tree (or branch) with this label and no children yet.
    pub fn new(label: impl Into<String>) -> Self {
        Tree {
            label: label.into(),
            children: Vec::new(),
        }
    }

    /// Add a branch with children of its own.
    pub fn child(mut self, tree: Tree) -> Self {
        self.children.push(tree);
        self
    }

    /// Add a child with no children.
    pub fn leaf(self, label: impl Into<String>) -> Self {
        self.child(Tree::new(label))
    }

    /// The tree as lines.
    pub fn lines(&self) -> Block {
        let mut out = vec![format!("  {}", paint(ROSE, &self.label))];
        self.render_children("", &mut out);
        out.into()
    }

    fn render_children(&self, prefix: &str, out: &mut Vec<String>) {
        for (i, child) in self.children.iter().enumerate() {
            let last = i == self.children.len() - 1;
            let color = if child.children.is_empty() { SOFT } else { TEXT };
            let guide = if last { "└─ " } else { "├─ " };
            out.push(format!(
                "  {}{}{}",
                paint(LINE, prefix),
                paint(LINE, guide),
                paint(color, &child.label)
            ));
            child.render_children(&format!("{prefix}{}", if last { "   " } else { "│  " }), out);
        }
    }

    /// Print the tree.
    pub fn print(&self) {
        self.lines().print();
    }
}

impl fmt::Display for Tree {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.lines().fmt(f)
    }
}
