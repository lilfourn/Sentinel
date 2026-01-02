//! Recursive descent parser for the rule DSL.
//!
//! Parses rule expressions like:
//! - `file.ext == 'pdf'`
//! - `file.ext IN ['pdf', 'docx']`
//! - `file.vector_similarity('tax invoice') > 0.8`
//! - `file.name.contains('invoice') AND file.size > 10KB`
//! - `NOT file.isHidden AND file.modifiedAt > '2024-01-01'`
//! - `(file.ext == 'jpg' OR file.ext == 'png') AND file.size < 5MB`

use super::ast::*;
use std::iter::Peekable;
use std::str::Chars;

/// Error type for rule parsing failures
#[derive(Debug, Clone, PartialEq)]
pub struct ParseError {
    pub message: String,
    pub position: usize,
}

impl ParseError {
    pub fn new(message: impl Into<String>, position: usize) -> Self {
        Self {
            message: message.into(),
            position,
        }
    }
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Parse error at position {}: {}", self.position, self.message)
    }
}

impl std::error::Error for ParseError {}

/// Token types for the lexer
#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    // Identifiers and literals
    Identifier(String),
    String(String),
    Number(f64),
    SizeBytes(u64),

    // Keywords
    And,
    Or,
    Not,
    In,
    Matches,
    True,
    False,

    // Operators
    Eq,         // ==
    Ne,         // !=
    Gt,         // >
    Lt,         // <
    Gte,        // >=
    Lte,        // <=

    // Punctuation
    Dot,        // .
    Comma,      // ,
    LParen,     // (
    RParen,     // )
    LBracket,   // [
    RBracket,   // ]

    // End of input
    Eof,
}

/// Lexer for tokenizing rule expressions
pub struct Lexer<'a> {
    input: &'a str,
    chars: Peekable<Chars<'a>>,
    position: usize,
}

impl<'a> Lexer<'a> {
    pub fn new(input: &'a str) -> Self {
        Self {
            input,
            chars: input.chars().peekable(),
            position: 0,
        }
    }

    fn advance(&mut self) -> Option<char> {
        let c = self.chars.next();
        if c.is_some() {
            self.position += 1;
        }
        c
    }

    fn peek(&mut self) -> Option<&char> {
        self.chars.peek()
    }

    fn skip_whitespace(&mut self) {
        while let Some(&c) = self.peek() {
            if c.is_whitespace() {
                self.advance();
            } else {
                break;
            }
        }
    }

    fn read_string(&mut self, quote: char) -> Result<Token, ParseError> {
        let start = self.position;
        let mut s = String::new();

        loop {
            match self.advance() {
                Some(c) if c == quote => return Ok(Token::String(s)),
                Some('\\') => {
                    // Handle escape sequences
                    match self.advance() {
                        Some('n') => s.push('\n'),
                        Some('t') => s.push('\t'),
                        Some('r') => s.push('\r'),
                        Some('\\') => s.push('\\'),
                        Some(c) if c == quote => s.push(c),
                        Some(c) => {
                            s.push('\\');
                            s.push(c);
                        }
                        None => return Err(ParseError::new("Unterminated string escape", self.position)),
                    }
                }
                Some(c) => s.push(c),
                None => return Err(ParseError::new("Unterminated string", start)),
            }
        }
    }

    fn read_number(&mut self, first: char) -> Result<Token, ParseError> {
        let start = self.position - 1;
        let mut s = String::new();
        s.push(first);

        // Read integer part
        while let Some(&c) = self.peek() {
            if c.is_ascii_digit() {
                s.push(self.advance().unwrap());
            } else {
                break;
            }
        }

        // Check for decimal part
        if let Some(&'.') = self.peek() {
            // Look ahead to see if it's a decimal or method call
            let rest = &self.input[self.position..];
            if rest.len() > 1 && rest.chars().nth(1).map_or(false, |c| c.is_ascii_digit()) {
                s.push(self.advance().unwrap()); // consume '.'
                while let Some(&c) = self.peek() {
                    if c.is_ascii_digit() {
                        s.push(self.advance().unwrap());
                    } else {
                        break;
                    }
                }
            }
        }

        // Check for size suffix (KB, MB, GB, TB)
        if let Some(&c) = self.peek() {
            if c.is_ascii_alphabetic() {
                let suffix_start = self.position;
                let mut suffix = String::new();
                while let Some(&c) = self.peek() {
                    if c.is_ascii_alphabetic() {
                        suffix.push(self.advance().unwrap());
                    } else {
                        break;
                    }
                }

                let multiplier = match suffix.to_uppercase().as_str() {
                    "B" => Some(1u64),
                    "KB" | "K" => Some(1024u64),
                    "MB" | "M" => Some(1024u64 * 1024),
                    "GB" | "G" => Some(1024u64 * 1024 * 1024),
                    "TB" | "T" => Some(1024u64 * 1024 * 1024 * 1024),
                    _ => None,
                };

                if let Some(mult) = multiplier {
                    let base: f64 = s.parse().map_err(|_| {
                        ParseError::new("Invalid number format", start)
                    })?;
                    return Ok(Token::SizeBytes((base * mult as f64) as u64));
                } else {
                    return Err(ParseError::new(
                        format!("Unknown size suffix: {}", suffix),
                        suffix_start,
                    ));
                }
            }
        }

        let n: f64 = s.parse().map_err(|_| {
            ParseError::new("Invalid number format", start)
        })?;
        Ok(Token::Number(n))
    }

    fn read_identifier(&mut self, first: char) -> Token {
        let mut s = String::new();
        s.push(first);

        while let Some(&c) = self.peek() {
            if c.is_alphanumeric() || c == '_' {
                s.push(self.advance().unwrap());
            } else {
                break;
            }
        }

        // Check for keywords
        match s.to_uppercase().as_str() {
            "AND" => Token::And,
            "OR" => Token::Or,
            "NOT" => Token::Not,
            "IN" => Token::In,
            "MATCHES" => Token::Matches,
            "TRUE" => Token::True,
            "FALSE" => Token::False,
            _ => Token::Identifier(s),
        }
    }

    pub fn next_token(&mut self) -> Result<Token, ParseError> {
        self.skip_whitespace();

        match self.advance() {
            None => Ok(Token::Eof),
            Some(c) => match c {
                // String literals
                '\'' | '"' => self.read_string(c),

                // Numbers
                c if c.is_ascii_digit() => self.read_number(c),

                // Identifiers and keywords
                c if c.is_alphabetic() || c == '_' => Ok(self.read_identifier(c)),

                // Two-character operators
                '=' => {
                    if let Some(&'=') = self.peek() {
                        self.advance();
                        Ok(Token::Eq)
                    } else {
                        Err(ParseError::new("Expected '==' operator", self.position - 1))
                    }
                }
                '!' => {
                    // Skip any whitespace between ! and = (AI models sometimes generate "! =")
                    while let Some(&c) = self.peek() {
                        if c == ' ' || c == '\t' {
                            self.advance();
                        } else {
                            break;
                        }
                    }

                    if let Some(&'=') = self.peek() {
                        self.advance();
                        Ok(Token::Ne)
                    } else {
                        Err(ParseError::new("Expected '!=' operator, or use NOT for negation", self.position - 1))
                    }
                }
                '>' => {
                    if let Some(&'=') = self.peek() {
                        self.advance();
                        Ok(Token::Gte)
                    } else {
                        Ok(Token::Gt)
                    }
                }
                '<' => {
                    if let Some(&'=') = self.peek() {
                        self.advance();
                        Ok(Token::Lte)
                    } else {
                        Ok(Token::Lt)
                    }
                }

                // Single-character tokens
                '.' => Ok(Token::Dot),
                ',' => Ok(Token::Comma),
                '(' => Ok(Token::LParen),
                ')' => Ok(Token::RParen),
                '[' => Ok(Token::LBracket),
                ']' => Ok(Token::RBracket),

                // Operators using symbols
                '&' => {
                    if let Some(&'&') = self.peek() {
                        self.advance();
                        Ok(Token::And)
                    } else {
                        Err(ParseError::new("Expected '&&' operator", self.position - 1))
                    }
                }
                '|' => {
                    if let Some(&'|') = self.peek() {
                        self.advance();
                        Ok(Token::Or)
                    } else {
                        Err(ParseError::new("Expected '||' operator", self.position - 1))
                    }
                }

                _ => Err(ParseError::new(
                    format!("Unexpected character: '{}'", c),
                    self.position - 1,
                )),
            },
        }
    }

    pub fn tokenize(&mut self) -> Result<Vec<Token>, ParseError> {
        let mut tokens = Vec::new();
        loop {
            let token = self.next_token()?;
            if token == Token::Eof {
                tokens.push(token);
                break;
            }
            tokens.push(token);
        }
        Ok(tokens)
    }
}

/// Recursive descent parser for rule expressions
pub struct RuleParser {
    tokens: Vec<Token>,
    position: usize,
}

impl RuleParser {
    /// Parse a rule expression string into an AST
    pub fn parse(input: &str) -> Result<Expression, ParseError> {
        let mut lexer = Lexer::new(input);
        let tokens = lexer.tokenize()?;

        let mut parser = Self { tokens, position: 0 };
        let expr = parser.parse_expression()?;

        // Ensure we consumed all tokens
        if !parser.is_at_end() {
            return Err(ParseError::new(
                format!("Unexpected token: {:?}", parser.current()),
                parser.position,
            ));
        }

        Ok(expr)
    }

    fn current(&self) -> &Token {
        self.tokens.get(self.position).unwrap_or(&Token::Eof)
    }

    fn advance(&mut self) -> &Token {
        if !self.is_at_end() {
            self.position += 1;
        }
        self.tokens.get(self.position - 1).unwrap_or(&Token::Eof)
    }

    fn is_at_end(&self) -> bool {
        matches!(self.current(), Token::Eof)
    }

    fn check(&self, token: &Token) -> bool {
        std::mem::discriminant(self.current()) == std::mem::discriminant(token)
    }

    fn consume(&mut self, expected: &Token, message: &str) -> Result<&Token, ParseError> {
        if self.check(expected) {
            Ok(self.advance())
        } else {
            Err(ParseError::new(
                format!("{}, got {:?}", message, self.current()),
                self.position,
            ))
        }
    }

    /// Parse expression: handles OR (lowest precedence)
    fn parse_expression(&mut self) -> Result<Expression, ParseError> {
        self.parse_or()
    }

    /// Parse OR expression
    fn parse_or(&mut self) -> Result<Expression, ParseError> {
        let mut left = self.parse_and()?;

        while matches!(self.current(), Token::Or) {
            self.advance();
            let right = self.parse_and()?;
            left = Expression::Or(Box::new(left), Box::new(right));
        }

        Ok(left)
    }

    /// Parse AND expression
    fn parse_and(&mut self) -> Result<Expression, ParseError> {
        let mut left = self.parse_not()?;

        while matches!(self.current(), Token::And) {
            self.advance();
            let right = self.parse_not()?;
            left = Expression::And(Box::new(left), Box::new(right));
        }

        Ok(left)
    }

    /// Parse NOT expression
    fn parse_not(&mut self) -> Result<Expression, ParseError> {
        if matches!(self.current(), Token::Not) {
            self.advance();
            let expr = self.parse_not()?;
            Ok(Expression::Not(Box::new(expr)))
        } else {
            self.parse_primary()
        }
    }

    /// Parse primary expression (atoms, parentheses, comparisons, function calls)
    fn parse_primary(&mut self) -> Result<Expression, ParseError> {
        match self.current().clone() {
            // Boolean literals
            Token::True => {
                self.advance();
                Ok(Expression::Literal(true))
            }
            Token::False => {
                self.advance();
                Ok(Expression::Literal(false))
            }

            // Parenthesized expression
            Token::LParen => {
                self.advance();
                let expr = self.parse_expression()?;
                self.consume(&Token::RParen, "Expected ')'")?;
                Ok(expr)
            }

            // Field access, comparison, or function call
            Token::Identifier(name) => {
                if name.to_lowercase() == "file" {
                    self.parse_file_expression()
                } else {
                    Err(ParseError::new(
                        format!("Expected 'file', got '{}'", name),
                        self.position,
                    ))
                }
            }

            _ => Err(ParseError::new(
                format!("Unexpected token: {:?}", self.current()),
                self.position,
            )),
        }
    }

    /// Parse file.field, file.field.function(), or file.function() expressions
    fn parse_file_expression(&mut self) -> Result<Expression, ParseError> {
        self.advance(); // consume 'file'
        self.consume(&Token::Dot, "Expected '.' after 'file'")?;

        // Get the field or function name
        let name = match self.current().clone() {
            Token::Identifier(n) => {
                self.advance();
                n
            }
            _ => {
                return Err(ParseError::new(
                    "Expected field name after 'file.'",
                    self.position,
                ));
            }
        };

        // Check if this is a direct function call on file (e.g., file.vector_similarity)
        if let Some(func_name) = FunctionName::from_str(&name) {
            // This is a function call: file.function(args)
            self.consume(&Token::LParen, "Expected '(' for function call")?;
            let args = self.parse_function_args()?;
            self.consume(&Token::RParen, "Expected ')'")?;

            // For vector_similarity, we need a comparison operator
            if matches!(func_name, FunctionName::VectorSimilarity) {
                if let Some(op) = self.try_parse_comparison_op() {
                    let value = self.parse_value()?;
                    // Return as a comparison where the left side is the function result
                    // We'll represent this specially
                    return Ok(Expression::And(
                        Box::new(Expression::FunctionCall(FunctionCall {
                            receiver: "file".to_string(),
                            function: func_name,
                            args,
                        })),
                        Box::new(Expression::Comparison(Comparison {
                            field: Field::FileName, // Placeholder, evaluator handles this specially
                            op,
                            value,
                        })),
                    ));
                } else {
                    // No comparison, just the function call (evaluates to a score)
                    return Ok(Expression::FunctionCall(FunctionCall {
                        receiver: "file".to_string(),
                        function: func_name,
                        args,
                    }));
                }
            }

            return Ok(Expression::FunctionCall(FunctionCall {
                receiver: "file".to_string(),
                function: func_name,
                args,
            }));
        }

        // This should be a field reference
        let field = Field::from_str(&name).ok_or_else(|| {
            ParseError::new(format!("Unknown field: '{}'", name), self.position - 1)
        })?;

        // Check for method chain: file.field.function()
        if matches!(self.current(), Token::Dot) {
            self.advance();
            let func_name = match self.current().clone() {
                Token::Identifier(n) => {
                    self.advance();
                    n
                }
                _ => {
                    return Err(ParseError::new(
                        "Expected function name after field",
                        self.position,
                    ));
                }
            };

            let function = FunctionName::from_str(&func_name).ok_or_else(|| {
                ParseError::new(format!("Unknown function: '{}'", func_name), self.position - 1)
            })?;

            self.consume(&Token::LParen, "Expected '(' for function call")?;
            let args = self.parse_function_args()?;
            self.consume(&Token::RParen, "Expected ')'")?;

            return Ok(Expression::FunctionCall(FunctionCall {
                receiver: format!("file.{}", field.canonical_name()),
                function,
                args,
            }));
        }

        // Check for comparison operator
        if let Some(op) = self.try_parse_comparison_op() {
            let value = self.parse_value()?;
            return Ok(Expression::Comparison(Comparison { field, op, value }));
        }

        // Check for IN operator
        if matches!(self.current(), Token::In) {
            self.advance();
            let value = self.parse_value()?;
            return Ok(Expression::Comparison(Comparison {
                field,
                op: ComparisonOp::In,
                value,
            }));
        }

        // Check for MATCHES operator
        if matches!(self.current(), Token::Matches) {
            self.advance();
            let value = self.parse_value()?;
            return Ok(Expression::Comparison(Comparison {
                field,
                op: ComparisonOp::Matches,
                value,
            }));
        }

        // For boolean fields, no operator means checking if true
        if matches!(field, Field::FileIsHidden) {
            return Ok(Expression::Comparison(Comparison {
                field,
                op: ComparisonOp::Eq,
                value: Value::Boolean(true),
            }));
        }

        Err(ParseError::new(
            "Expected comparison operator, IN, or MATCHES",
            self.position,
        ))
    }

    fn try_parse_comparison_op(&mut self) -> Option<ComparisonOp> {
        let op = match self.current() {
            Token::Eq => Some(ComparisonOp::Eq),
            Token::Ne => Some(ComparisonOp::Ne),
            Token::Gt => Some(ComparisonOp::Gt),
            Token::Lt => Some(ComparisonOp::Lt),
            Token::Gte => Some(ComparisonOp::Gte),
            Token::Lte => Some(ComparisonOp::Lte),
            _ => None,
        };
        if op.is_some() {
            self.advance();
        }
        op
    }

    fn parse_function_args(&mut self) -> Result<Vec<Value>, ParseError> {
        let mut args = Vec::new();

        // Empty args
        if matches!(self.current(), Token::RParen) {
            return Ok(args);
        }

        args.push(self.parse_value()?);

        while matches!(self.current(), Token::Comma) {
            self.advance();
            args.push(self.parse_value()?);
        }

        Ok(args)
    }

    fn parse_value(&mut self) -> Result<Value, ParseError> {
        match self.current().clone() {
            Token::String(s) => {
                self.advance();
                Ok(Value::String(s))
            }
            Token::Number(n) => {
                self.advance();
                Ok(Value::Number(n))
            }
            Token::SizeBytes(b) => {
                self.advance();
                Ok(Value::SizeBytes(b))
            }
            Token::True => {
                self.advance();
                Ok(Value::Boolean(true))
            }
            Token::False => {
                self.advance();
                Ok(Value::Boolean(false))
            }
            Token::LBracket => {
                self.advance();
                let mut arr = Vec::new();

                if !matches!(self.current(), Token::RBracket) {
                    arr.push(self.parse_value()?);
                    while matches!(self.current(), Token::Comma) {
                        self.advance();
                        arr.push(self.parse_value()?);
                    }
                }

                self.consume(&Token::RBracket, "Expected ']'")?;
                Ok(Value::Array(arr))
            }
            _ => Err(ParseError::new(
                format!("Expected value, got {:?}", self.current()),
                self.position,
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_comparison() {
        let expr = RuleParser::parse("file.ext == 'pdf'").unwrap();
        match expr {
            Expression::Comparison(cmp) => {
                assert_eq!(cmp.field, Field::FileExt);
                assert_eq!(cmp.op, ComparisonOp::Eq);
                assert_eq!(cmp.value, Value::String("pdf".to_string()));
            }
            _ => panic!("Expected comparison"),
        }
    }

    #[test]
    fn test_in_operator() {
        let expr = RuleParser::parse("file.ext IN ['pdf', 'docx']").unwrap();
        match expr {
            Expression::Comparison(cmp) => {
                assert_eq!(cmp.field, Field::FileExt);
                assert_eq!(cmp.op, ComparisonOp::In);
                if let Value::Array(arr) = cmp.value {
                    assert_eq!(arr.len(), 2);
                } else {
                    panic!("Expected array value");
                }
            }
            _ => panic!("Expected comparison"),
        }
    }

    #[test]
    fn test_size_literal() {
        let expr = RuleParser::parse("file.size > 10KB").unwrap();
        match expr {
            Expression::Comparison(cmp) => {
                assert_eq!(cmp.field, Field::FileSize);
                assert_eq!(cmp.op, ComparisonOp::Gt);
                assert_eq!(cmp.value, Value::SizeBytes(10 * 1024));
            }
            _ => panic!("Expected comparison"),
        }
    }

    #[test]
    fn test_and_expression() {
        let expr = RuleParser::parse("file.ext == 'pdf' AND file.size > 1MB").unwrap();
        match expr {
            Expression::And(left, right) => {
                assert!(matches!(*left, Expression::Comparison(_)));
                assert!(matches!(*right, Expression::Comparison(_)));
            }
            _ => panic!("Expected AND expression"),
        }
    }

    #[test]
    fn test_or_expression() {
        let expr = RuleParser::parse("file.ext == 'jpg' OR file.ext == 'png'").unwrap();
        match expr {
            Expression::Or(_, _) => {}
            _ => panic!("Expected OR expression"),
        }
    }

    #[test]
    fn test_not_expression() {
        let expr = RuleParser::parse("NOT file.isHidden").unwrap();
        match expr {
            Expression::Not(inner) => {
                assert!(matches!(*inner, Expression::Comparison(_)));
            }
            _ => panic!("Expected NOT expression"),
        }
    }

    #[test]
    fn test_parentheses() {
        let expr = RuleParser::parse("(file.ext == 'jpg' OR file.ext == 'png') AND file.size < 5MB").unwrap();
        match expr {
            Expression::And(left, right) => {
                assert!(matches!(*left, Expression::Or(_, _)));
                assert!(matches!(*right, Expression::Comparison(_)));
            }
            _ => panic!("Expected AND expression with OR inside"),
        }
    }

    #[test]
    fn test_function_call() {
        let expr = RuleParser::parse("file.name.contains('invoice')").unwrap();
        match expr {
            Expression::FunctionCall(func) => {
                assert_eq!(func.receiver, "file.name");
                assert_eq!(func.function, FunctionName::Contains);
                assert_eq!(func.args.len(), 1);
            }
            _ => panic!("Expected function call"),
        }
    }

    #[test]
    fn test_complex_expression() {
        let input = "file.name.contains('invoice') AND file.size > 10KB";
        let expr = RuleParser::parse(input).unwrap();
        assert!(matches!(expr, Expression::And(_, _)));
    }

    #[test]
    fn test_boolean_operators_symbols() {
        let expr = RuleParser::parse("file.ext == 'pdf' && file.size > 1KB").unwrap();
        assert!(matches!(expr, Expression::And(_, _)));

        let expr = RuleParser::parse("file.ext == 'jpg' || file.ext == 'png'").unwrap();
        assert!(matches!(expr, Expression::Or(_, _)));
    }

    #[test]
    fn test_inequality_with_spaces() {
        // AI models sometimes generate "! =" with a space - parser should tolerate this
        let expr = RuleParser::parse("file.ext ! = 'pdf'").unwrap();
        match expr {
            Expression::Comparison(cmp) => {
                assert_eq!(cmp.op, ComparisonOp::Ne);
            }
            _ => panic!("Expected comparison expression"),
        }

        // Multiple spaces should also work
        let expr = RuleParser::parse("file.name !  = 'test'").unwrap();
        assert!(matches!(expr, Expression::Comparison(_)));

        // Tabs should also work
        let expr = RuleParser::parse("file.ext !\t= 'doc'").unwrap();
        assert!(matches!(expr, Expression::Comparison(_)));
    }
}
