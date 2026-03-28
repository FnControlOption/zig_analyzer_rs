use super::*;

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
        if !self.documents.contains_key(&path) {
            self.documents.insert(path.clone(), 'doc: {
                let Ok(source) = std::fs::read(&path) else {
                    break 'doc None;
                };
                let Some(tree) = Ast::parse(source) else {
                    break 'doc None;
                };
                Some(Document::new(Rc::new(tree)))
            });
        }
        self.documents.get(&path).unwrap().as_ref()
    }

    pub fn enclosing_container(
        &self,
        handle: &Handle,
        token_index: TokenIndex,
    ) -> Option<(NodeIndex, Rc<Scope>)> {
        let Some(document) = self.get(handle.path()) else {
            return None;
        };
        assert!(Rc::ptr_eq(document.tree(), handle.tree()));
        document.enclosing_container(token_index)
    }
}

pub struct Document {
    tree: Rc<Ast>,
    scopes: OrderMap<NodeIndex, Rc<Scope>>,
}

impl Document {
    pub fn new(tree: Rc<Ast>) -> Self {
        let mut scopes = OrderMap::new();
        for i in 0..tree.node_count() {
            if let Some(scope) = Scope::new(&tree, NodeIndex(i)) {
                scopes.insert(NodeIndex(i), Rc::new(scope));
            }
        }
        scopes.sort_by_key(|&node_index, _| {
            let first: u32 = tree.first_token(node_index).0;
            let last: u32 = tree.last_token(node_index).0;
            (first, u32::MAX - last)
        });
        Self { tree, scopes }
    }

    pub fn tree(&self) -> &Rc<Ast> {
        &self.tree
    }

    pub fn get(&self, node_index: NodeIndex) -> Option<Rc<Scope>> {
        self.scopes.get(&node_index).cloned()
    }

    pub fn enclosing_scopes(
        &self,
        token_index: TokenIndex,
    ) -> impl Iterator<Item = (NodeIndex, Rc<Scope>)> {
        self.scopes
            .iter()
            .take_while(move |&(&node_index, _)| {
                let first_token = self.tree.first_token(node_index);
                token_index >= first_token
            })
            .filter(move |&(&node_index, _)| {
                let last_token = self.tree.last_token(node_index);
                token_index <= last_token
            })
            .map(|(&node_index, scope)| (node_index, scope.clone()))
    }

    pub fn enclosing_container(&self, token_index: TokenIndex) -> Option<(NodeIndex, Rc<Scope>)> {
        let mut enclosing = None;
        for (node_index, scope) in self.enclosing_scopes(token_index) {
            if is_container_decl(&self.tree, node_index) {
                enclosing = Some((node_index, scope));
            }
        }
        enclosing
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Declaration {
    Variable(NodeIndex),
    Function(NodeIndex),
}

impl Declaration {
    pub fn node_index(self) -> NodeIndex {
        match self {
            Declaration::Variable(node_index) => node_index,
            Declaration::Function(node_index) => node_index,
        }
    }
}

#[derive(Debug)]
pub struct Scope {
    pub label: Option<Vec<u8>>,
    pub fields: OrderMap<Vec<u8>, NodeIndex>,
    pub decls: OrderMap<Vec<u8>, Declaration>,
}

impl Scope {
    pub fn new(tree: &Ast, node_index: NodeIndex) -> Option<Self> {
        let mut scope = Self {
            label: None,
            fields: OrderMap::new(),
            decls: OrderMap::new(),
        };
        match tree.node_tag(node_index) {
            NodeTag::Root
            | NodeTag::ContainerDecl
            | NodeTag::ContainerDeclTrailing
            | NodeTag::ContainerDeclTwo
            | NodeTag::ContainerDeclTwoTrailing
            | NodeTag::ContainerDeclArg
            | NodeTag::ContainerDeclArgTrailing
            | NodeTag::TaggedUnion
            | NodeTag::TaggedUnionTrailing
            | NodeTag::TaggedUnionTwo
            | NodeTag::TaggedUnionTwoTrailing
            | NodeTag::TaggedUnionEnumTag
            | NodeTag::TaggedUnionEnumTagTrailing => {
                let buffered = tree.full_node_buffered(node_index).unwrap();
                let container_decl: &full::ContainerDecl = buffered.get();
                scope.load_members(tree, container_decl.ast.members());
                scope.load_optional_member(tree, container_decl.ast.arg);
                if scope.fields.is_empty() && scope.decls.is_empty() {
                    return None;
                }
                Some(scope)
            }
            // TODO: other scopes
            _ => None,
        }
    }

    fn load_members(&mut self, tree: &Ast, children: &[NodeIndex]) {
        for &child in children {
            self.load_member(tree, child);
        }
    }

    fn load_optional_member(&mut self, tree: &Ast, child: OptionalNodeIndex) {
        if let Some(node_index) = child.to_option() {
            self.load_member(tree, node_index);
        }
    }

    fn load_member(&mut self, tree: &Ast, node_index: NodeIndex) {
        match tree.node_tag(node_index) {
            NodeTag::ContainerFieldInit
            | NodeTag::ContainerFieldAlign
            | NodeTag::ContainerField => {
                let container_field: full::ContainerField = tree.full_node(node_index).unwrap();
                let name_token = container_field.ast.main_token;
                let name = tree.token_slice(name_token);
                self.fields.insert(Vec::from(name), node_index);
            }
            NodeTag::GlobalVarDecl
            | NodeTag::LocalVarDecl
            | NodeTag::SimpleVarDecl
            | NodeTag::AlignedVarDecl => {
                let var_decl: full::VarDecl = tree.full_node(node_index).unwrap();
                let mut name_token = var_decl.ast.mut_token;
                name_token.0 += 1;
                if name_token.0 >= tree.token_count() {
                    return;
                }
                let name = tree.token_slice(name_token);
                let decl = Declaration::Variable(node_index);
                self.decls.insert(Vec::from(name), decl);
            }
            NodeTag::FnProtoSimple
            | NodeTag::FnProtoMulti
            | NodeTag::FnProtoOne
            | NodeTag::FnProto
            | NodeTag::FnDecl => {
                let fn_proto_buf = tree.full_node_buffered(node_index).unwrap();
                let fn_proto: &full::FnProto = fn_proto_buf.get();
                let Some(name_token) = fn_proto.name_token.to_option() else {
                    return;
                };
                let name = tree.token_slice(name_token);
                let decl = Declaration::Function(node_index);
                self.decls.insert(Vec::from(name), decl);
            }
            _ => {}
        }
    }
}

fn is_container_decl(tree: &Ast, node_index: NodeIndex) -> bool {
    // TODO: check node tag instead
    let Some(buffered) = tree.full_node_buffered(node_index) else {
        return false;
    };
    let _: &full::ContainerDecl = buffered.get();
    true
}
