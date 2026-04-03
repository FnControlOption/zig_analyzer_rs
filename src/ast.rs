use super::*;

pub struct TokenIterator {
    it: TokenIndexIterator,
}

impl TokenIterator {
    pub fn new(tree: &Ast, start: TokenIndex) -> Self {
        let end = TokenIndex(tree.token_count());
        let it = TokenIndexIterator::from_range(start, end);
        Self { it }
    }

    pub fn peek(&mut self) -> Option<TokenIndex> {
        self.it.peek()
    }

    pub fn next(&mut self) -> Option<TokenIndex> {
        self.it.next()
    }

    pub fn consume(&mut self, tree: &Ast, tag: TokenTag) -> Option<TokenIndex> {
        if tree.token_tag(self.peek()?) == tag {
            return self.next();
        }
        None
    }

    pub fn payload(&mut self, tree: &Ast) -> Option<(bool, TokenIndex)> {
        if let Some(name_token) = self.consume(tree, TokenTag::Identifier) {
            return Some((false, name_token));
        }
        self.consume(tree, TokenTag::Asterisk)?;
        let name_token = self.consume(tree, TokenTag::Identifier)?;
        return Some((true, name_token));
    }
}
