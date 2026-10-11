//! Content models of element declarations: the scaffold that `doProlog`
//! builds while it reads a declaration (`CONTENT_SCAFFOLD`,
//! `nextScaffoldPart`) and the model given to the element declaration
//! handler (`XML_Content`, `build_model`).
//!
//! The model is a flat array like C's: node 0 is the root and the
//! children of a node are a contiguous range of later nodes, in C's order
//! (siblings adjacent, breadth-first per parent). Neither building it nor
//! dropping it recurses, so a deeply nested model cannot exhaust the
//! stack (C made `build_model` iterative for the same reason).

use alloc::vec::Vec;
use core::ops::Range;

/// `enum XML_Content_Type`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ContentType {
    /// `XML_CTYPE_EMPTY`
    Empty = 1,
    /// `XML_CTYPE_ANY`
    Any,
    /// `XML_CTYPE_MIXED`
    Mixed,
    /// `XML_CTYPE_NAME`
    Name,
    /// `XML_CTYPE_CHOICE`
    Choice,
    /// `XML_CTYPE_SEQ`
    #[default]
    Seq,
}

/// `enum XML_Content_Quant`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ContentQuant {
    /// `XML_CQUANT_NONE`
    #[default]
    None = 0,
    /// `XML_CQUANT_OPT` (`?`)
    Opt,
    /// `XML_CQUANT_REP` (`*`)
    Rep,
    /// `XML_CQUANT_PLUS` (`+`)
    Plus,
}

/// One node of a [`ContentModel`] (`XML_Content`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContentNode {
    /// `type`
    pub kind: ContentType,
    /// `quant`
    pub quant: ContentQuant,
    /// `name` (only for [`ContentType::Name`]).
    pub name: Option<Vec<u8>>,
    /// `children[0..numchildren]`, as indices into [`ContentModel::nodes`].
    pub children: Range<usize>,
}

/// The content model of an element declaration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContentModel {
    /// The nodes; node 0 is the root.
    pub nodes: Vec<ContentNode>,
}

impl ContentModel {
    /// The model of `EMPTY` or `ANY`: a single node.
    pub(crate) fn leaf(kind: ContentType) -> Self {
        ContentModel {
            nodes: alloc::vec![ContentNode {
                kind,
                quant: ContentQuant::None,
                name: None,
                children: 0..0,
            }],
        }
    }

    /// The root node.
    pub fn root(&self) -> Option<&ContentNode> {
        self.nodes.first()
    }

    /// The children of `node`.
    pub fn children(&self, node: &ContentNode) -> &[ContentNode] {
        self.nodes.get(node.children.clone()).unwrap_or_default()
    }
}

/// `CONTENT_SCAFFOLD`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Scaffold {
    /// `type`
    pub kind: ContentType,
    /// `quant`
    pub quant: ContentQuant,
    /// `name`
    pub name: Option<Vec<u8>>,
    /// `firstchild`
    pub firstchild: usize,
    /// `lastchild` (0: none; node 0 is the root and never a child).
    pub lastchild: usize,
    /// `childcnt`
    pub childcnt: usize,
    /// `nextsib`
    pub nextsib: usize,
}

impl super::Dtd {
    /// `nextScaffoldPart`: a new scaffold node, appended to the children of
    /// the innermost open group. Like C, the array is reused between
    /// declarations; a reused node keeps its old type, quantifier and name
    /// until the caller sets them.
    pub(crate) fn next_scaffold_part(&mut self) -> usize {
        let next = self.scaff_count;
        self.scaff_count += 1;
        if self.scaffold.len() < self.scaff_count {
            self.scaffold.push(Scaffold::default());
        }
        if self.scaff_level > 0 {
            let parent = self
                .scaff_index
                .get(self.scaff_level - 1)
                .copied()
                .unwrap_or(0);
            let last = self.scaffold.get(parent).map_or(0, |p| p.lastchild);
            if last != 0
                && let Some(l) = self.scaffold.get_mut(last)
            {
                l.nextsib = next;
            }
            if let Some(p) = self.scaffold.get_mut(parent) {
                if p.childcnt == 0 {
                    p.firstchild = next;
                }
                p.lastchild = next;
                p.childcnt += 1;
            }
        }
        if let Some(me) = self.scaffold.get_mut(next) {
            me.firstchild = 0;
            me.lastchild = 0;
            me.childcnt = 0;
            me.nextsib = 0;
        }
        next
    }

    /// `build_model`: the model of `scaffold[0..scaffCount]`, in C's node
    /// order.
    pub(crate) fn build_model(&self) -> ContentModel {
        let count = self.scaff_count;
        let mut nodes = Vec::with_capacity(count);
        // Source index of each destination node ("jobs" in C).
        let mut jobs: Vec<usize> = Vec::with_capacity(count);
        jobs.push(0);
        let mut dest = 0;
        while dest < count {
            let src = jobs.get(dest).copied().unwrap_or(0);
            let Some(sc) = self.scaffold.get(src) else {
                break;
            };
            if sc.kind == ContentType::Name {
                nodes.push(ContentNode {
                    kind: sc.kind,
                    quant: sc.quant,
                    name: Some(sc.name.clone().unwrap_or_default()),
                    children: 0..0,
                });
            } else {
                let first = jobs.len();
                let mut cn = sc.firstchild;
                for _ in 0..sc.childcnt {
                    jobs.push(cn);
                    cn = self.scaffold.get(cn).map_or(0, |c| c.nextsib);
                }
                nodes.push(ContentNode {
                    kind: sc.kind,
                    quant: sc.quant,
                    name: None,
                    children: first..first + sc.childcnt,
                });
            }
            dest += 1;
        }
        ContentModel { nodes }
    }
}
