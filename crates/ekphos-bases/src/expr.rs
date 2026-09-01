use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    Reference(String),
    Member(Box<Expr>, String),
    Index(Box<Expr>, Box<Expr>),
    Call(Box<Expr>, Vec<Expr>),
    Unary { op: UnaryOp, value: Box<Expr> },
    Binary { left: Box<Expr>, op: BinaryOp, right: Box<Expr> },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOp {
    Not,
    Negate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOp {
    Or,
    And,
    Equal,
    NotEqual,
    Greater,
    GreaterEqual,
    Less,
    LessEqual,
    Add,
    Subtract,
    Multiply,
    Divide,
    Modulo,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExprError {
    pub offset: usize,
    pub message: String,
}

impl fmt::Display for ExprError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{} at byte {}", self.message, self.offset)
    }
}

impl std::error::Error for ExprError {}

#[derive(Debug, Clone, PartialEq)]
enum TokenKind {
    Ident(String),
    Number(f64),
    String(String),
    True,
    False,
    Null,
    LeftParen,
    RightParen,
    LeftBracket,
    RightBracket,
    Dot,
    Comma,
    Or,
    And,
    Equal,
    NotEqual,
    Greater,
    GreaterEqual,
    Less,
    LessEqual,
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    Bang,
    End,
}

#[derive(Debug, Clone, PartialEq)]
struct Token {
    kind: TokenKind,
    offset: usize,
}

pub fn parse_expression(source: &str) -> Result<Expr, ExprError> {
    let tokens = Lexer::new(source).tokenize()?;
    let mut parser = Parser { tokens, cursor: 0, depth: 0 };
    let expression = parser.parse_precedence(0)?;
    if !matches!(parser.current().kind, TokenKind::End) {
        return Err(parser.error("unexpected trailing input"));
    }
    Ok(expression)
}

struct Lexer<'a> {
    source: &'a str,
    cursor: usize,
}

impl<'a> Lexer<'a> {
    fn new(source: &'a str) -> Self {
        Self { source, cursor: 0 }
    }

    fn tokenize(mut self) -> Result<Vec<Token>, ExprError> {
        const MAX_TOKENS: usize = 4096;
        let mut tokens = Vec::new();
        while self.cursor < self.source.len() {
            if tokens.len() >= MAX_TOKENS {
                return Err(self.error("expression is too complex"));
            }
            let offset = self.cursor;
            let character = self.peek().expect("cursor checked");
            if character.is_whitespace() {
                self.bump();
                continue;
            }
            let kind = match character {
                '(' => self.single(TokenKind::LeftParen),
                ')' => self.single(TokenKind::RightParen),
                '[' => self.single(TokenKind::LeftBracket),
                ']' => self.single(TokenKind::RightBracket),
                '.' => self.single(TokenKind::Dot),
                ',' => self.single(TokenKind::Comma),
                '+' => self.single(TokenKind::Plus),
                '-' => self.single(TokenKind::Minus),
                '*' => self.single(TokenKind::Star),
                '/' => self.single(TokenKind::Slash),
                '%' => self.single(TokenKind::Percent),
                '!' => {
                    self.bump();
                    if self.take('=') {
                        TokenKind::NotEqual
                    } else {
                        TokenKind::Bang
                    }
                }
                '=' => {
                    self.bump();
                    if self.take('=') {
                        TokenKind::Equal
                    } else {
                        return Err(ExprError { offset, message: "expected '=='".to_string() });
                    }
                }
                '>' => {
                    self.bump();
                    if self.take('=') {
                        TokenKind::GreaterEqual
                    } else {
                        TokenKind::Greater
                    }
                }
                '<' => {
                    self.bump();
                    if self.take('=') {
                        TokenKind::LessEqual
                    } else {
                        TokenKind::Less
                    }
                }
                '&' => {
                    self.bump();
                    if self.take('&') {
                        TokenKind::And
                    } else {
                        return Err(ExprError { offset, message: "expected '&&'".to_string() });
                    }
                }
                '|' => {
                    self.bump();
                    if self.take('|') {
                        TokenKind::Or
                    } else {
                        return Err(ExprError { offset, message: "expected '||'".to_string() });
                    }
                }
                '\'' | '"' => TokenKind::String(self.string(character)?),
                value if value.is_ascii_digit() => TokenKind::Number(self.number()?),
                value if is_identifier_start(value) => {
                    let identifier = self.identifier();
                    match identifier.as_str() {
                        "true" => TokenKind::True,
                        "false" => TokenKind::False,
                        "null" => TokenKind::Null,
                        _ => TokenKind::Ident(identifier),
                    }
                }
                _ => return Err(self.error("unsupported character")),
            };
            tokens.push(Token { kind, offset });
        }
        tokens.push(Token { kind: TokenKind::End, offset: self.source.len() });
        Ok(tokens)
    }

    fn peek(&self) -> Option<char> {
        self.source[self.cursor..].chars().next()
    }

    fn bump(&mut self) -> Option<char> {
        let character = self.peek()?;
        self.cursor += character.len_utf8();
        Some(character)
    }

    fn take(&mut self, expected: char) -> bool {
        if self.peek() == Some(expected) {
            self.bump();
            true
        } else {
            false
        }
    }

    fn single(&mut self, kind: TokenKind) -> TokenKind {
        self.bump();
        kind
    }

    fn identifier(&mut self) -> String {
        let start = self.cursor;
        while self.peek().is_some_and(is_identifier_continue) {
            self.bump();
        }
        self.source[start..self.cursor].to_string()
    }

    fn number(&mut self) -> Result<f64, ExprError> {
        let start = self.cursor;
        while self.peek().is_some_and(|character| character.is_ascii_digit() || character == '.') {
            self.bump();
        }
        self.source[start..self.cursor].parse().map_err(|_| ExprError { offset: start, message: "invalid number".to_string() })
    }

    fn string(&mut self, quote: char) -> Result<String, ExprError> {
        let start = self.cursor;
        self.bump();
        let mut output = String::new();
        while let Some(character) = self.bump() {
            if character == quote {
                return Ok(output);
            }
            if character == '\\' {
                let escaped = self.bump().ok_or_else(|| ExprError { offset: start, message: "unterminated string".to_string() })?;
                output.push(match escaped {
                    'n' => '\n',
                    'r' => '\r',
                    't' => '\t',
                    other => other,
                });
            } else {
                output.push(character);
            }
        }
        Err(ExprError { offset: start, message: "unterminated string".to_string() })
    }

    fn error(&self, message: &str) -> ExprError {
        ExprError { offset: self.cursor, message: message.to_string() }
    }
}

fn is_identifier_start(character: char) -> bool {
    character == '_' || character.is_alphabetic()
}

fn is_identifier_continue(character: char) -> bool {
    character == '_' || character.is_alphanumeric()
}

struct Parser {
    tokens: Vec<Token>,
    cursor: usize,
    depth: usize,
}

impl Parser {
    fn parse_precedence(&mut self, minimum: u8) -> Result<Expr, ExprError> {
        const MAX_DEPTH: usize = 128;
        if self.depth >= MAX_DEPTH {
            return Err(self.error("expression nesting is too deep"));
        }
        self.depth += 1;
        let mut left = self.parse_prefix()?;
        while let Some((precedence, operator)) = binary_operator(&self.current().kind) {
            if precedence < minimum {
                break;
            }
            self.advance();
            let right = self.parse_precedence(precedence + 1)?;
            left = Expr::Binary { left: Box::new(left), op: operator, right: Box::new(right) };
        }
        self.depth -= 1;
        Ok(left)
    }

    fn parse_prefix(&mut self) -> Result<Expr, ExprError> {
        let mut expression = match self.current().kind.clone() {
            TokenKind::Null => {
                self.advance();
                Expr::Null
            }
            TokenKind::True => {
                self.advance();
                Expr::Bool(true)
            }
            TokenKind::False => {
                self.advance();
                Expr::Bool(false)
            }
            TokenKind::Number(value) => {
                self.advance();
                Expr::Number(value)
            }
            TokenKind::String(value) => {
                self.advance();
                Expr::String(value)
            }
            TokenKind::Ident(identifier) => {
                self.advance();
                Expr::Reference(identifier)
            }
            TokenKind::Bang => {
                self.advance();
                Expr::Unary { op: UnaryOp::Not, value: Box::new(self.parse_precedence(8)?) }
            }
            TokenKind::Minus => {
                self.advance();
                Expr::Unary { op: UnaryOp::Negate, value: Box::new(self.parse_precedence(8)?) }
            }
            TokenKind::LeftParen => {
                self.advance();
                let expression = self.parse_precedence(0)?;
                self.expect(|kind| matches!(kind, TokenKind::RightParen), "expected ')'")?;
                expression
            }
            _ => return Err(self.error("expected a value")),
        };
        loop {
            expression = match self.current().kind.clone() {
                TokenKind::Dot => {
                    self.advance();
                    let TokenKind::Ident(member) = self.current().kind.clone() else {
                        return Err(self.error("expected a property or method name after '.'"));
                    };
                    self.advance();
                    Expr::Member(Box::new(expression), member)
                }
                TokenKind::LeftBracket => {
                    self.advance();
                    let index = self.parse_precedence(0)?;
                    self.expect(|kind| matches!(kind, TokenKind::RightBracket), "expected ']'")?;
                    Expr::Index(Box::new(expression), Box::new(index))
                }
                TokenKind::LeftParen => {
                    self.advance();
                    let mut arguments = Vec::new();
                    if !matches!(self.current().kind, TokenKind::RightParen) {
                        loop {
                            arguments.push(self.parse_precedence(0)?);
                            if !matches!(self.current().kind, TokenKind::Comma) {
                                break;
                            }
                            self.advance();
                        }
                    }
                    self.expect(|kind| matches!(kind, TokenKind::RightParen), "expected ')' after arguments")?;
                    Expr::Call(Box::new(expression), arguments)
                }
                _ => break,
            };
        }
        Ok(expression)
    }

    fn expect(&mut self, predicate: impl FnOnce(&TokenKind) -> bool, message: &str) -> Result<(), ExprError> {
        if predicate(&self.current().kind) {
            self.advance();
            Ok(())
        } else {
            Err(self.error(message))
        }
    }

    fn current(&self) -> &Token {
        &self.tokens[self.cursor]
    }

    fn advance(&mut self) {
        self.cursor = (self.cursor + 1).min(self.tokens.len() - 1);
    }

    fn error(&self, message: &str) -> ExprError {
        ExprError { offset: self.current().offset, message: message.to_string() }
    }
}

fn binary_operator(kind: &TokenKind) -> Option<(u8, BinaryOp)> {
    Some(match kind {
        TokenKind::Or => (1, BinaryOp::Or),
        TokenKind::And => (2, BinaryOp::And),
        TokenKind::Equal => (3, BinaryOp::Equal),
        TokenKind::NotEqual => (3, BinaryOp::NotEqual),
        TokenKind::Greater => (4, BinaryOp::Greater),
        TokenKind::GreaterEqual => (4, BinaryOp::GreaterEqual),
        TokenKind::Less => (4, BinaryOp::Less),
        TokenKind::LessEqual => (4, BinaryOp::LessEqual),
        TokenKind::Plus => (5, BinaryOp::Add),
        TokenKind::Minus => (5, BinaryOp::Subtract),
        TokenKind::Star => (6, BinaryOp::Multiply),
        TokenKind::Slash => (6, BinaryOp::Divide),
        TokenKind::Percent => (6, BinaryOp::Modulo),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_obsidian_style_expression_shapes() {
        for source in ["status != \"done\" && price > 2.1", "file.hasTag(\"book\")", "(price / age).toFixed(2)", "note[\"hyphenated-property\"]", "if(due, date(due) < today() + \"7d\", false)", "price-age"] {
            parse_expression(source).unwrap_or_else(|error| panic!("{source}: {error}"));
        }
        let Expr::Call(_, arguments) = parse_expression("if(true, 1, 2)").unwrap() else { panic!("expected a call") };
        assert_eq!(arguments, vec![Expr::Bool(true), Expr::Number(1.0), Expr::Number(2.0)]);
    }

    #[test]
    fn rejects_unbounded_and_malformed_input() {
        assert!(parse_expression("price = 2").is_err());
        assert!(parse_expression("file.hasTag(\"book\"").is_err());
        assert!(parse_expression(&"!".repeat(200)).is_err());
    }
}
