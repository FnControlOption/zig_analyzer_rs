use ordermap::OrderMap;
use rangemap::RangeMap;

use super::*;

fn is_container_decl(tree: &Ast, node_index: NodeIndex) -> bool {
    // TODO: check node tag instead
    let Some(buffered) = tree.full_node_buffered(node_index) else {
        return false;
    };
    let _: &full::ContainerDecl = buffered.get();
    true
}

pub struct DocumentStore {
    documents: HashMap<Rc<Path>, Option<Document>>,
}

impl DocumentStore {
    pub fn new() -> Self {
        Self {
            documents: HashMap::new(),
        }
    }

    pub fn insert(&mut self, path: Rc<Path>, document: Document) {
        self.documents.insert(path, Some(document));
    }

    pub fn get(&self, path: &Path) -> Option<&Document> {
        self.documents.get(path).unwrap_or(&None).as_ref()
    }

    pub fn get_or_parse(&mut self, path: Rc<Path>) -> Option<&Document> {
        match self.documents.contains_key(&path) {
            true => self.documents.get(&path).unwrap().as_ref(),
            false => self.parse(path.clone(), None),
        }
    }

    pub fn parse(&mut self, path: Rc<Path>, source: Option<Vec<u8>>) -> Option<&Document> {
        self.documents.insert(path.clone(), 'blk: {
            let source = match source {
                Some(source) => source,
                None => match std::fs::read(&path) {
                    Ok(source) => source,
                    Err(_) => break 'blk None,
                },
            };
            let Some(tree) = Ast::parse(source) else {
                break 'blk None;
            };
            Some(Document::new(Rc::new(tree)))
        });
        self.documents.get(&path).unwrap().as_ref()
    }

    pub fn enclosing_container<'doc>(
        &'doc self,
        handle: &Handle,
        token_index: TokenIndex,
    ) -> Option<&'doc DocumentNode> {
        let Some(document) = self.get(handle.path()) else {
            return None;
        };
        assert!(Rc::ptr_eq(document.tree(), handle.tree()));
        document.enclosing_container(token_index)
    }
}

pub struct Document {
    tree: Rc<Ast>,
    root: DocumentNode,
}

impl Document {
    pub fn new(tree: Rc<Ast>) -> Self {
        let builder = DocumentNodeBuilder::new(NodeIndex::ROOT);
        let root = builder.build(&tree);
        Self { tree, root }
    }

    pub fn tree(&self) -> &Rc<Ast> {
        &self.tree
    }

    pub fn get(&self, node_index: NodeIndex) -> &DocumentNode {
        if node_index.is_root() {
            return &self.root;
        }

        let first_token = self.tree.first_token(node_index);
        let start = self.tree.token_start(first_token);

        let mut node = &self.root;
        while let Some(child) = node.children.get(&start) {
            node = child;
        }
        assert_eq!(node.index, node_index);
        node
    }

    /// Returns an iterator of nodes that enclose the given token, from biggest
    /// to smallest. Includes the `root` node.
    ///
    /// Also see: [`DocumentNode::enclosing_nodes`]
    pub fn enclosing_nodes(&self, token_index: TokenIndex) -> impl Iterator<Item = &DocumentNode> {
        self.root.enclosing_nodes(&self.tree, token_index)
    }

    /// Returns the smallest container decl that contains the given token.
    pub fn enclosing_container(&self, token_index: TokenIndex) -> Option<&DocumentNode> {
        let mut enclosing = None;
        for node in self.enclosing_nodes(token_index) {
            if is_container_decl(&self.tree, node.index) {
                enclosing = Some(node);
            }
        }
        enclosing
    }

    pub fn position_to_token(&self, line: u32, character: u32) -> TokenIndex {
        let source = self.tree.source();
        let mut source_index = 0;
        for _ in 0..line {
            let slice = &source[source_index as usize..];
            match slice.iter().position(|&c| c == b'\n') {
                Some(n) => {
                    source_index += n as u32;
                    source_index += 1;
                }
                None => break,
            }
        }
        source_index += character;
        self.source_index_to_token(source_index)
    }

    pub fn source_index_to_token(&self, source_index: u32) -> TokenIndex {
        // TODO: optimize this
        // https://github.com/zigtools/zls/blob/ef64fa0/src/offsets.zig#L121
        let mut current_token = TokenIndex(0);
        loop {
            let next_token = TokenIndex(current_token.0 + 1);
            if next_token.0 >= self.tree.token_count() {
                return current_token;
            }
            if self.tree.token_start(next_token) > source_index {
                return current_token;
            }
            current_token = next_token;
        }
    }
}

#[derive(Clone)]
pub struct DocumentNode {
    pub index: NodeIndex,
    pub children: RangeMap<u32, DocumentNode>,
    pub scope: Option<Rc<Scope>>,
}

impl DocumentNode {
    /// Returns an iterator of nodes that enclose the given token, from biggest
    /// to smallest. Includes `self`.
    pub fn enclosing_nodes(
        &self,
        tree: &Ast,
        token_index: TokenIndex,
    ) -> impl Iterator<Item = &DocumentNode> {
        EnclosingNodes {
            token_start: tree.token_start(token_index),
            next: Some(self),
        }
    }
}

impl PartialEq for DocumentNode {
    fn eq(&self, other: &Self) -> bool {
        self.index == other.index
    }
}

impl Eq for DocumentNode {}

struct DocumentNodeBuilder {
    index: NodeIndex,
    children: RangeMap<u32, DocumentNode>,
    members: OrderMap<Vec<u8>, Member>,
}

impl DocumentNodeBuilder {
    fn new(index: NodeIndex) -> Self {
        Self {
            index,
            children: RangeMap::new(),
            members: OrderMap::new(),
        }
    }

    fn build(mut self, tree: &Ast) -> DocumentNode {
        let index = self.index;
        if let Some(fn_proto_buf) = tree.full_node_buffered(index) {
            let fn_proto: &full::FnProto = fn_proto_buf.get();
            for &param in fn_proto.ast.params() {
                let first_token = tree.first_token(param);
                if first_token.0 < 2 {
                    continue;
                }
                let colon_token = TokenIndex(first_token.0 - 1);
                if tree.token_tag(colon_token) != TokenTag::Colon {
                    continue;
                }
                let name_token = TokenIndex(first_token.0 - 2);
                if tree.token_tag(name_token) != TokenTag::Identifier {
                    continue;
                }
                let name = tree.token_slice(name_token);
                let member = Member::FunctionParameter(param);
                self.members.insert(Vec::from(name), member);
            }
        }
        visit(&mut self, tree, index);
        let label = None; // TODO
        let members = self.members;
        DocumentNode {
            index,
            children: self.children,
            scope: match label.is_some() || members.len() > 0 {
                true => Some(Rc::new(Scope { label, members })),
                false => None,
            },
        }
    }
}

impl Visit for DocumentNodeBuilder {
    fn visit(&mut self, tree: &Ast, index: NodeIndex) {
        let first_token = tree.first_token(index);
        let last_token = tree.last_token(index);

        let start = tree.token_start(first_token);
        let end = tree.token_start(last_token) + tree.token_length(last_token);

        let builder = DocumentNodeBuilder::new(index);
        let child = builder.build(tree);

        self.children.insert(start..end, child);

        match tree.node_tag(index) {
            NodeTag::ContainerFieldInit
            | NodeTag::ContainerFieldAlign
            | NodeTag::ContainerField => {
                let container_field: full::ContainerField = tree.full_node(index).unwrap();
                let name_token = container_field.ast.main_token;
                let name = tree.token_slice(name_token);
                let member = Member::Field(index);
                self.members.insert(Vec::from(name), member);
            }
            NodeTag::GlobalVarDecl
            | NodeTag::LocalVarDecl
            | NodeTag::SimpleVarDecl
            | NodeTag::AlignedVarDecl => {
                let var_decl: full::VarDecl = tree.full_node(index).unwrap();
                let mut name_token = var_decl.ast.mut_token;
                name_token.0 += 1;
                if name_token.0 >= tree.token_count() {
                    return;
                }
                let name = tree.token_slice(name_token);
                let member = Member::Variable(index);
                self.members.insert(Vec::from(name), member);
            }
            NodeTag::FnProtoSimple
            | NodeTag::FnProtoMulti
            | NodeTag::FnProtoOne
            | NodeTag::FnProto
            | NodeTag::FnDecl => {
                let fn_proto_buf = tree.full_node_buffered(index).unwrap();
                let fn_proto: &full::FnProto = fn_proto_buf.get();
                let Some(name_token) = fn_proto.name_token.to_option() else {
                    return;
                };
                let name = tree.token_slice(name_token);
                let member = Member::Function(index);
                self.members.insert(Vec::from(name), member);
            }
            _ => {}
        }
    }
}

#[derive(Debug)]
pub struct Scope {
    pub label: Option<Vec<u8>>,
    pub members: OrderMap<Vec<u8>, Member>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Member {
    Field(NodeIndex),
    Variable(NodeIndex),
    Function(NodeIndex),
    FunctionParameter(NodeIndex),
}

impl Member {
    pub fn name_token(self, tree: &Ast) -> TokenIndex {
        match self {
            Member::Field(node_index) => tree.node_main_token(node_index),
            Member::Variable(node_index) => {
                let mut_token = tree.node_main_token(node_index);
                TokenIndex(mut_token.0 + 1)
            }
            Member::Function(node_index) => {
                let fn_token = tree.node_main_token(node_index);
                TokenIndex(fn_token.0 + 1)
            }
            Member::FunctionParameter(node_index) => {
                let first_token = tree.first_token(node_index);
                TokenIndex(first_token.0 - 2)
            }
        }
    }

    pub fn def_slice(self, tree: &Ast) -> &[u8] {
        match self {
            Member::Field(node_index) => tree.node_source(node_index),
            Member::Variable(node_index) => tree.node_source(node_index),
            Member::Function(node_index) => {
                let buffered = tree.full_node_buffered(node_index).unwrap();
                let fn_proto: &full::FnProto = buffered.get();
                tree.node_source(fn_proto.ast.proto_node)
            }
            Member::FunctionParameter(node_index) => {
                let mut first_token = tree.first_token(node_index);
                first_token.0 -= 2;
                let last_token = tree.last_token(node_index);

                let start = tree.token_start(first_token);
                let end = tree.token_start(last_token) + tree.token_length(last_token);

                &tree.source()[start as usize..end as usize]
            }
        }
    }
}

struct EnclosingNodes<'doc> {
    token_start: u32,
    next: Option<&'doc DocumentNode>,
}

impl<'doc> Iterator for EnclosingNodes<'doc> {
    type Item = &'doc DocumentNode;

    fn next(&mut self) -> Option<Self::Item> {
        let next = self.next;
        if let Some(next) = next {
            self.next = next.children.get(&self.token_start);
        }
        next
    }
}
