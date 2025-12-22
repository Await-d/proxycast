//! 过滤表达式解析器
//!
//! 该模块实现类似 mitmproxy 的过滤表达式语法，支持组合条件过滤 Flow。
//!
//! # 支持的过滤器
//!
//! - `~m <pattern>`: 模型名称匹配
//! - `~p <provider>`: 提供商匹配
//! - `~s <state>`: 状态匹配 (pending/streaming/completed/failed)
//! - `~e`: 有错误
//! - `~t`: 有工具调用
//! - `~k`: 有思维链
//! - `~starred`: 已收藏
//! - `~tag <name>`: 包含标签
//! - `~b <regex>`: 请求或响应内容匹配
//! - `~bq <regex>`: 请求内容匹配
//! - `~bs <regex>`: 响应内容匹配
//! - `~tokens <op> <n>`: Token 数量比较
//! - `~latency <op> <n>`: 延迟比较 (支持 s/ms 后缀)
//! - `&`: AND 逻辑
//! - `|`: OR 逻辑
//! - `!`: NOT 逻辑
//! - `()`: 分组

use regex::Regex;
use serde::{Deserialize, Serialize};
use std::fmt;
use thiserror::Error;

use super::models::{FlowState, LLMFlow, MessageContent};

// ============================================================================
// 错误类型
// ============================================================================

/// 过滤表达式解析错误
#[derive(Debug, Clone, Error, PartialEq, Eq, Serialize, Deserialize)]
pub enum FilterParseError {
    /// 意外的字符
    #[error("意外的字符 '{0}' 在位置 {1}")]
    UnexpectedChar(char, usize),

    /// 意外的 Token
    #[error("意外的 Token '{0}' 在位置 {1}")]
    UnexpectedToken(String, usize),

    /// 意外的输入结束
    #[error("意外的输入结束")]
    UnexpectedEof,

    /// 未知的过滤器类型
    #[error("未知的过滤器类型 '{0}'")]
    UnknownFilter(String),

    /// 缺少参数
    #[error("过滤器 '{0}' 缺少参数")]
    MissingArgument(String),

    /// 无效的比较运算符
    #[error("无效的比较运算符 '{0}'")]
    InvalidComparisonOp(String),

    /// 无效的数值
    #[error("无效的数值 '{0}'")]
    InvalidNumber(String),

    /// 无效的状态值
    #[error("无效的状态值 '{0}'，有效值: pending, streaming, completed, failed, cancelled")]
    InvalidState(String),

    /// 无效的正则表达式
    #[error("无效的正则表达式: {0}")]
    InvalidRegex(String),

    /// 括号不匹配
    #[error("括号不匹配")]
    UnmatchedParen,

    /// 空表达式
    #[error("空表达式")]
    EmptyExpression,
}

// ============================================================================
// 比较运算符
// ============================================================================

/// 比较运算符
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ComparisonOp {
    /// 大于
    Gt,
    /// 大于等于
    Gte,
    /// 小于
    Lt,
    /// 小于等于
    Lte,
    /// 等于
    Eq,
}

impl fmt::Display for ComparisonOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ComparisonOp::Gt => write!(f, ">"),
            ComparisonOp::Gte => write!(f, ">="),
            ComparisonOp::Lt => write!(f, "<"),
            ComparisonOp::Lte => write!(f, "<="),
            ComparisonOp::Eq => write!(f, "="),
        }
    }
}

/// 数值比较
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Comparison {
    pub op: ComparisonOp,
    pub value: i64,
}

impl Comparison {
    /// 执行比较
    pub fn compare(&self, actual: i64) -> bool {
        match self.op {
            ComparisonOp::Gt => actual > self.value,
            ComparisonOp::Gte => actual >= self.value,
            ComparisonOp::Lt => actual < self.value,
            ComparisonOp::Lte => actual <= self.value,
            ComparisonOp::Eq => actual == self.value,
        }
    }
}

impl fmt::Display for Comparison {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}{}", self.op, self.value)
    }
}

// ============================================================================
// Token 类型
// ============================================================================

/// 过滤表达式 Token
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum FilterToken {
    // 基础过滤器
    /// 模型名称匹配 (~m <pattern>)
    Model(String),
    /// 提供商匹配 (~p <provider>)
    Provider(String),
    /// 状态匹配 (~s <state>)
    State(FlowState),
    /// 有错误 (~e)
    HasError,
    /// 有工具调用 (~t)
    HasToolCalls,
    /// 有思维链 (~k)
    HasThinking,
    /// 已收藏 (~starred)
    Starred,
    /// 包含标签 (~tag <name>)
    Tag(String),

    // 内容搜索
    /// 请求或响应内容匹配 (~b <regex>)
    Body(String),
    /// 请求内容匹配 (~bq <regex>)
    BodyRequest(String),
    /// 响应内容匹配 (~bs <regex>)
    BodyResponse(String),

    // 数值比较
    /// Token 数量比较 (~tokens <op> <value>)
    Tokens(Comparison),
    /// 延迟比较 (~latency <op> <value>)
    Latency(Comparison),

    // 逻辑运算
    /// AND 逻辑 (&)
    And,
    /// OR 逻辑 (|)
    Or,
    /// NOT 逻辑 (!)
    Not,

    // 分组
    /// 左括号 (
    LeftParen,
    /// 右括号 )
    RightParen,
}

impl fmt::Display for FilterToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FilterToken::Model(s) => write!(f, "~m {}", s),
            FilterToken::Provider(s) => write!(f, "~p {}", s),
            FilterToken::State(s) => write!(f, "~s {}", state_to_string(s)),
            FilterToken::HasError => write!(f, "~e"),
            FilterToken::HasToolCalls => write!(f, "~t"),
            FilterToken::HasThinking => write!(f, "~k"),
            FilterToken::Starred => write!(f, "~starred"),
            FilterToken::Tag(s) => write!(f, "~tag {}", s),
            FilterToken::Body(s) => write!(f, "~b {}", s),
            FilterToken::BodyRequest(s) => write!(f, "~bq {}", s),
            FilterToken::BodyResponse(s) => write!(f, "~bs {}", s),
            FilterToken::Tokens(c) => write!(f, "~tokens {}", c),
            FilterToken::Latency(c) => write!(f, "~latency {}", c),
            FilterToken::And => write!(f, "&"),
            FilterToken::Or => write!(f, "|"),
            FilterToken::Not => write!(f, "!"),
            FilterToken::LeftParen => write!(f, "("),
            FilterToken::RightParen => write!(f, ")"),
        }
    }
}

/// 将 FlowState 转换为字符串
fn state_to_string(state: &FlowState) -> &'static str {
    match state {
        FlowState::Pending => "pending",
        FlowState::Streaming => "streaming",
        FlowState::Completed => "completed",
        FlowState::Failed => "failed",
        FlowState::Cancelled => "cancelled",
    }
}

/// 从字符串解析 FlowState
fn parse_state(s: &str) -> Result<FlowState, FilterParseError> {
    match s.to_lowercase().as_str() {
        "pending" => Ok(FlowState::Pending),
        "streaming" => Ok(FlowState::Streaming),
        "completed" => Ok(FlowState::Completed),
        "failed" => Ok(FlowState::Failed),
        "cancelled" => Ok(FlowState::Cancelled),
        _ => Err(FilterParseError::InvalidState(s.to_string())),
    }
}

// ============================================================================
// AST 表达式
// ============================================================================

/// 过滤表达式 AST
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum FilterExpr {
    /// 单个 Token
    Token(FilterToken),
    /// AND 表达式
    And(Box<FilterExpr>, Box<FilterExpr>),
    /// OR 表达式
    Or(Box<FilterExpr>, Box<FilterExpr>),
    /// NOT 表达式
    Not(Box<FilterExpr>),
}

impl fmt::Display for FilterExpr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FilterExpr::Token(t) => write!(f, "{}", t),
            FilterExpr::And(left, right) => write!(f, "({} & {})", left, right),
            FilterExpr::Or(left, right) => write!(f, "({} | {})", left, right),
            FilterExpr::Not(expr) => write!(f, "!{}", expr),
        }
    }
}

// ============================================================================
// 词法分析器 (Lexer)
// ============================================================================

/// 词法分析器
struct Lexer<'a> {
    input: &'a str,
    chars: std::iter::Peekable<std::str::CharIndices<'a>>,
    pos: usize,
}

impl<'a> Lexer<'a> {
    fn new(input: &'a str) -> Self {
        Self {
            input,
            chars: input.char_indices().peekable(),
            pos: 0,
        }
    }

    /// 跳过空白字符
    fn skip_whitespace(&mut self) {
        while let Some(&(_, c)) = self.chars.peek() {
            if c.is_whitespace() {
                self.chars.next();
            } else {
                break;
            }
        }
    }

    /// 读取一个单词（字母数字和下划线、连字符）
    fn read_word(&mut self) -> String {
        let mut word = String::new();
        while let Some(&(_, c)) = self.chars.peek() {
            if c.is_alphanumeric() || c == '_' || c == '-' || c == '.' || c == '*' {
                word.push(c);
                self.chars.next();
            } else {
                break;
            }
        }
        word
    }

    /// 读取带引号的字符串
    fn read_quoted_string(&mut self, quote: char) -> Result<String, FilterParseError> {
        let mut s = String::new();
        // 跳过开始引号
        self.chars.next();

        while let Some((pos, c)) = self.chars.next() {
            if c == quote {
                return Ok(s);
            } else if c == '\\' {
                // 转义字符
                if let Some((_, next_c)) = self.chars.next() {
                    s.push(next_c);
                } else {
                    return Err(FilterParseError::UnexpectedEof);
                }
            } else {
                s.push(c);
            }
            self.pos = pos;
        }
        Err(FilterParseError::UnexpectedEof)
    }

    /// 读取参数（可能带引号或不带引号）
    fn read_argument(&mut self) -> Result<String, FilterParseError> {
        self.skip_whitespace();

        if let Some(&(_, c)) = self.chars.peek() {
            if c == '"' || c == '\'' {
                return self.read_quoted_string(c);
            }
        }

        let word = self.read_word();
        if word.is_empty() {
            return Err(FilterParseError::UnexpectedEof);
        }
        Ok(word)
    }

    /// 解析比较运算符和数值
    fn parse_comparison(&mut self, filter_name: &str) -> Result<Comparison, FilterParseError> {
        self.skip_whitespace();

        // 读取运算符
        let op = match self.chars.peek() {
            Some(&(_, '>')) => {
                self.chars.next();
                if let Some(&(_, '=')) = self.chars.peek() {
                    self.chars.next();
                    ComparisonOp::Gte
                } else {
                    ComparisonOp::Gt
                }
            }
            Some(&(_, '<')) => {
                self.chars.next();
                if let Some(&(_, '=')) = self.chars.peek() {
                    self.chars.next();
                    ComparisonOp::Lte
                } else {
                    ComparisonOp::Lt
                }
            }
            Some(&(_, '=')) => {
                self.chars.next();
                ComparisonOp::Eq
            }
            Some(&(pos, c)) => {
                return Err(FilterParseError::InvalidComparisonOp(c.to_string()));
            }
            None => {
                return Err(FilterParseError::MissingArgument(filter_name.to_string()));
            }
        };

        self.skip_whitespace();

        // 读取数值（可能带单位）
        let value_str = self.read_word();
        if value_str.is_empty() {
            return Err(FilterParseError::MissingArgument(filter_name.to_string()));
        }

        let value = self.parse_value_with_unit(&value_str, filter_name)?;

        Ok(Comparison { op, value })
    }

    /// 解析带单位的数值
    fn parse_value_with_unit(&self, s: &str, filter_name: &str) -> Result<i64, FilterParseError> {
        let s = s.to_lowercase();

        // 检查是否有单位后缀
        if filter_name == "latency" {
            if let Some(num_str) = s.strip_suffix("ms") {
                return num_str
                    .parse::<i64>()
                    .map_err(|_| FilterParseError::InvalidNumber(s.clone()));
            } else if let Some(num_str) = s.strip_suffix('s') {
                return num_str
                    .parse::<i64>()
                    .map(|n| n * 1000)
                    .map_err(|_| FilterParseError::InvalidNumber(s.clone()));
            }
        }

        // 尝试直接解析为数字
        s.parse::<i64>()
            .map_err(|_| FilterParseError::InvalidNumber(s))
    }

    /// 解析过滤器 Token
    fn parse_filter(&mut self) -> Result<FilterToken, FilterParseError> {
        self.skip_whitespace();

        // 读取过滤器名称
        let filter_name = self.read_word();

        match filter_name.as_str() {
            "m" => {
                let pattern = self.read_argument()?;
                Ok(FilterToken::Model(pattern))
            }
            "p" => {
                let provider = self.read_argument()?;
                Ok(FilterToken::Provider(provider))
            }
            "s" => {
                let state_str = self.read_argument()?;
                let state = parse_state(&state_str)?;
                Ok(FilterToken::State(state))
            }
            "e" => Ok(FilterToken::HasError),
            "t" => Ok(FilterToken::HasToolCalls),
            "k" => Ok(FilterToken::HasThinking),
            "starred" => Ok(FilterToken::Starred),
            "tag" => {
                let tag = self.read_argument()?;
                Ok(FilterToken::Tag(tag))
            }
            "b" => {
                let pattern = self.read_argument()?;
                // 验证正则表达式
                Regex::new(&pattern).map_err(|e| FilterParseError::InvalidRegex(e.to_string()))?;
                Ok(FilterToken::Body(pattern))
            }
            "bq" => {
                let pattern = self.read_argument()?;
                Regex::new(&pattern).map_err(|e| FilterParseError::InvalidRegex(e.to_string()))?;
                Ok(FilterToken::BodyRequest(pattern))
            }
            "bs" => {
                let pattern = self.read_argument()?;
                Regex::new(&pattern).map_err(|e| FilterParseError::InvalidRegex(e.to_string()))?;
                Ok(FilterToken::BodyResponse(pattern))
            }
            "tokens" => {
                let comparison = self.parse_comparison("tokens")?;
                Ok(FilterToken::Tokens(comparison))
            }
            "latency" => {
                let comparison = self.parse_comparison("latency")?;
                Ok(FilterToken::Latency(comparison))
            }
            _ => Err(FilterParseError::UnknownFilter(filter_name)),
        }
    }

    /// 获取下一个 Token
    fn next_token(&mut self) -> Result<Option<FilterToken>, FilterParseError> {
        self.skip_whitespace();

        match self.chars.peek() {
            None => Ok(None),
            Some(&(pos, c)) => {
                self.pos = pos;
                match c {
                    '~' => {
                        self.chars.next();
                        let token = self.parse_filter()?;
                        Ok(Some(token))
                    }
                    '&' => {
                        self.chars.next();
                        Ok(Some(FilterToken::And))
                    }
                    '|' => {
                        self.chars.next();
                        Ok(Some(FilterToken::Or))
                    }
                    '!' => {
                        self.chars.next();
                        Ok(Some(FilterToken::Not))
                    }
                    '(' => {
                        self.chars.next();
                        Ok(Some(FilterToken::LeftParen))
                    }
                    ')' => {
                        self.chars.next();
                        Ok(Some(FilterToken::RightParen))
                    }
                    _ => Err(FilterParseError::UnexpectedChar(c, pos)),
                }
            }
        }
    }

    /// 词法分析，返回所有 Token
    fn tokenize(&mut self) -> Result<Vec<FilterToken>, FilterParseError> {
        let mut tokens = Vec::new();
        while let Some(token) = self.next_token()? {
            tokens.push(token);
        }
        Ok(tokens)
    }
}

// ============================================================================
// 语法分析器 (Parser)
// ============================================================================

/// 语法分析器
struct Parser {
    tokens: Vec<FilterToken>,
    pos: usize,
}

impl Parser {
    fn new(tokens: Vec<FilterToken>) -> Self {
        Self { tokens, pos: 0 }
    }

    /// 查看当前 Token
    fn peek(&self) -> Option<&FilterToken> {
        self.tokens.get(self.pos)
    }

    /// 消费当前 Token
    fn advance(&mut self) -> Option<FilterToken> {
        if self.pos < self.tokens.len() {
            let token = self.tokens[self.pos].clone();
            self.pos += 1;
            Some(token)
        } else {
            None
        }
    }

    /// 检查当前 Token 是否匹配
    fn check(&self, token: &FilterToken) -> bool {
        self.peek().map_or(false, |t| {
            std::mem::discriminant(t) == std::mem::discriminant(token)
        })
    }

    /// 解析表达式
    fn parse_expr(&mut self) -> Result<FilterExpr, FilterParseError> {
        self.parse_or()
    }

    /// 解析 OR 表达式
    fn parse_or(&mut self) -> Result<FilterExpr, FilterParseError> {
        let mut left = self.parse_and()?;

        while self.check(&FilterToken::Or) {
            self.advance(); // 消费 |
            let right = self.parse_and()?;
            left = FilterExpr::Or(Box::new(left), Box::new(right));
        }

        Ok(left)
    }

    /// 解析 AND 表达式
    fn parse_and(&mut self) -> Result<FilterExpr, FilterParseError> {
        let mut left = self.parse_unary()?;

        while self.check(&FilterToken::And) {
            self.advance(); // 消费 &
            let right = self.parse_unary()?;
            left = FilterExpr::And(Box::new(left), Box::new(right));
        }

        Ok(left)
    }

    /// 解析一元表达式 (NOT)
    fn parse_unary(&mut self) -> Result<FilterExpr, FilterParseError> {
        if self.check(&FilterToken::Not) {
            self.advance(); // 消费 !
            let expr = self.parse_unary()?;
            return Ok(FilterExpr::Not(Box::new(expr)));
        }

        self.parse_primary()
    }

    /// 解析基本表达式
    fn parse_primary(&mut self) -> Result<FilterExpr, FilterParseError> {
        match self.peek() {
            Some(FilterToken::LeftParen) => {
                self.advance(); // 消费 (
                let expr = self.parse_expr()?;

                // 期望 )
                match self.peek() {
                    Some(FilterToken::RightParen) => {
                        self.advance();
                        Ok(expr)
                    }
                    _ => Err(FilterParseError::UnmatchedParen),
                }
            }
            Some(token) => {
                // 检查是否是过滤器 Token
                match token {
                    FilterToken::And | FilterToken::Or | FilterToken::RightParen => Err(
                        FilterParseError::UnexpectedToken(format!("{}", token), self.pos),
                    ),
                    _ => {
                        let token = self.advance().unwrap();
                        Ok(FilterExpr::Token(token))
                    }
                }
            }
            None => Err(FilterParseError::UnexpectedEof),
        }
    }
}

// ============================================================================
// FilterParser 公共接口
// ============================================================================

/// 过滤表达式解析器
pub struct FilterParser;

impl FilterParser {
    /// 解析过滤表达式字符串
    pub fn parse(input: &str) -> Result<FilterExpr, FilterParseError> {
        let input = input.trim();
        if input.is_empty() {
            return Err(FilterParseError::EmptyExpression);
        }

        let mut lexer = Lexer::new(input);
        let tokens = lexer.tokenize()?;

        if tokens.is_empty() {
            return Err(FilterParseError::EmptyExpression);
        }

        let mut parser = Parser::new(tokens);
        let expr = parser.parse_expr()?;

        // 检查是否还有未消费的 Token
        if parser.peek().is_some() {
            return Err(FilterParseError::UnexpectedToken(
                format!("{}", parser.peek().unwrap()),
                parser.pos,
            ));
        }

        Ok(expr)
    }

    /// 验证表达式语法
    pub fn validate(input: &str) -> Result<(), FilterParseError> {
        Self::parse(input)?;
        Ok(())
    }

    /// 将 FilterExpr 编译为可执行的过滤函数
    pub fn compile(expr: &FilterExpr) -> Box<dyn Fn(&LLMFlow) -> bool + Send + Sync> {
        let expr = expr.clone();
        Box::new(move |flow| Self::evaluate(&expr, flow))
    }

    /// 评估表达式
    fn evaluate(expr: &FilterExpr, flow: &LLMFlow) -> bool {
        match expr {
            FilterExpr::Token(token) => Self::evaluate_token(token, flow),
            FilterExpr::And(left, right) => {
                Self::evaluate(left, flow) && Self::evaluate(right, flow)
            }
            FilterExpr::Or(left, right) => {
                Self::evaluate(left, flow) || Self::evaluate(right, flow)
            }
            FilterExpr::Not(inner) => !Self::evaluate(inner, flow),
        }
    }

    /// 评估单个 Token
    fn evaluate_token(token: &FilterToken, flow: &LLMFlow) -> bool {
        match token {
            FilterToken::Model(pattern) => Self::match_pattern(pattern, &flow.request.model),
            FilterToken::Provider(provider) => {
                let flow_provider = format!("{:?}", flow.metadata.provider).to_lowercase();
                flow_provider.contains(&provider.to_lowercase())
            }
            FilterToken::State(state) => flow.state == *state,
            FilterToken::HasError => flow.error.is_some(),
            FilterToken::HasToolCalls => flow
                .response
                .as_ref()
                .map_or(false, |r| !r.tool_calls.is_empty()),
            FilterToken::HasThinking => flow
                .response
                .as_ref()
                .map_or(false, |r| r.thinking.is_some()),
            FilterToken::Starred => flow.annotations.starred,
            FilterToken::Tag(tag) => flow
                .annotations
                .tags
                .iter()
                .any(|t| t.to_lowercase() == tag.to_lowercase()),
            FilterToken::Body(pattern) => {
                let request_text = Self::get_request_text(flow);
                let response_text = flow
                    .response
                    .as_ref()
                    .map_or(String::new(), |r| r.content.clone());
                let combined = format!("{}\n{}", request_text, response_text);

                if let Ok(re) = Regex::new(pattern) {
                    re.is_match(&combined)
                } else {
                    combined.to_lowercase().contains(&pattern.to_lowercase())
                }
            }
            FilterToken::BodyRequest(pattern) => {
                let request_text = Self::get_request_text(flow);

                if let Ok(re) = Regex::new(pattern) {
                    re.is_match(&request_text)
                } else {
                    request_text
                        .to_lowercase()
                        .contains(&pattern.to_lowercase())
                }
            }
            FilterToken::BodyResponse(pattern) => {
                let response_text = flow
                    .response
                    .as_ref()
                    .map_or(String::new(), |r| r.content.clone());

                if let Ok(re) = Regex::new(pattern) {
                    re.is_match(&response_text)
                } else {
                    response_text
                        .to_lowercase()
                        .contains(&pattern.to_lowercase())
                }
            }
            FilterToken::Tokens(comparison) => {
                let total_tokens = flow
                    .response
                    .as_ref()
                    .map_or(0, |r| r.usage.total_tokens as i64);
                comparison.compare(total_tokens)
            }
            FilterToken::Latency(comparison) => {
                comparison.compare(flow.timestamps.duration_ms as i64)
            }
            // 逻辑运算符和括号不应该在这里出现
            FilterToken::And
            | FilterToken::Or
            | FilterToken::Not
            | FilterToken::LeftParen
            | FilterToken::RightParen => false,
        }
    }

    /// 模式匹配（支持 * 通配符）
    fn match_pattern(pattern: &str, text: &str) -> bool {
        if pattern == "*" {
            return true;
        }

        let pattern_lower = pattern.to_lowercase();
        let text_lower = text.to_lowercase();

        if pattern.contains('*') {
            // 通配符匹配
            let parts: Vec<&str> = pattern_lower.split('*').collect();
            let mut pos = 0;

            for (i, part) in parts.iter().enumerate() {
                if part.is_empty() {
                    continue;
                }

                if let Some(found_pos) = text_lower[pos..].find(part) {
                    // 第一个部分必须从开头匹配（如果模式不以 * 开头）
                    if i == 0 && found_pos != 0 && !pattern_lower.starts_with('*') {
                        return false;
                    }
                    pos += found_pos + part.len();
                } else {
                    return false;
                }
            }

            // 最后一个部分必须匹配到结尾（如果模式不以 * 结尾）
            if !pattern_lower.ends_with('*') && pos != text_lower.len() {
                return false;
            }

            true
        } else {
            // 不含通配符时，检查是否包含该模式
            text_lower.contains(&pattern_lower)
        }
    }

    /// 获取请求文本（用于搜索）
    fn get_request_text(flow: &LLMFlow) -> String {
        let mut text = String::new();

        if let Some(ref system) = flow.request.system_prompt {
            text.push_str(system);
            text.push('\n');
        }

        for msg in &flow.request.messages {
            match &msg.content {
                MessageContent::Text(s) => {
                    text.push_str(s);
                    text.push('\n');
                }
                MessageContent::MultiModal(parts) => {
                    for part in parts {
                        if let super::models::ContentPart::Text { text: t } = part {
                            text.push_str(t);
                            text.push('\n');
                        }
                    }
                }
            }
        }

        text
    }
}

// ============================================================================
// 帮助信息
// ============================================================================

/// 过滤表达式帮助信息
pub const FILTER_HELP: &[(&str, &str)] = &[
    ("~m <pattern>", "模型名称匹配（支持 * 通配符）"),
    ("~p <provider>", "提供商匹配"),
    (
        "~s <state>",
        "状态匹配 (pending/streaming/completed/failed/cancelled)",
    ),
    ("~e", "有错误"),
    ("~t", "有工具调用"),
    ("~k", "有思维链"),
    ("~starred", "已收藏"),
    ("~tag <name>", "包含标签"),
    ("~b <regex>", "请求或响应内容匹配（正则表达式）"),
    ("~bq <regex>", "请求内容匹配（正则表达式）"),
    ("~bs <regex>", "响应内容匹配（正则表达式）"),
    ("~tokens <op> <n>", "Token 数量比较 (>, >=, <, <=, =)"),
    ("~latency <op> <n>", "延迟比较 (支持 s/ms 后缀)"),
    ("&", "AND 逻辑"),
    ("|", "OR 逻辑"),
    ("!", "NOT 逻辑"),
    ("()", "分组"),
];

/// 获取帮助文本
pub fn get_filter_help() -> String {
    let mut help = String::from("过滤表达式语法:\n\n");
    for (syntax, desc) in FILTER_HELP {
        help.push_str(&format!("  {:<20} {}\n", syntax, desc));
    }
    help.push_str("\n示例:\n");
    help.push_str("  ~m claude              模型名称包含 'claude'\n");
    help.push_str("  ~p kiro & ~m claude    提供商为 kiro 且模型包含 claude\n");
    help.push_str("  ~e | ~latency >5s      有错误或延迟超过 5 秒\n");
    help.push_str("  !~e                    没有错误\n");
    help.push_str("  (~p kiro | ~p gemini) & ~tokens >1000\n");
    help
}

// ============================================================================
// 单元测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flow_monitor::models::{
        FlowAnnotations, FlowMetadata, FlowTimestamps, FlowType, LLMRequest, LLMResponse,
        RequestParameters, TokenUsage,
    };
    use crate::ProviderType;

    /// 创建测试用的 Flow
    fn create_test_flow(model: &str, provider: ProviderType) -> LLMFlow {
        let request = LLMRequest {
            method: "POST".to_string(),
            path: "/v1/chat/completions".to_string(),
            model: model.to_string(),
            parameters: RequestParameters::default(),
            ..Default::default()
        };

        let metadata = FlowMetadata {
            provider,
            ..Default::default()
        };

        LLMFlow::new(
            "test-id".to_string(),
            FlowType::ChatCompletions,
            request,
            metadata,
        )
    }

    #[test]
    fn test_parse_model_filter() {
        let expr = FilterParser::parse("~m claude").unwrap();
        assert!(matches!(expr, FilterExpr::Token(FilterToken::Model(s)) if s == "claude"));
    }

    #[test]
    fn test_parse_provider_filter() {
        let expr = FilterParser::parse("~p kiro").unwrap();
        assert!(matches!(expr, FilterExpr::Token(FilterToken::Provider(s)) if s == "kiro"));
    }

    #[test]
    fn test_parse_state_filter() {
        let expr = FilterParser::parse("~s completed").unwrap();
        assert!(matches!(
            expr,
            FilterExpr::Token(FilterToken::State(FlowState::Completed))
        ));
    }

    #[test]
    fn test_parse_has_error_filter() {
        let expr = FilterParser::parse("~e").unwrap();
        assert!(matches!(expr, FilterExpr::Token(FilterToken::HasError)));
    }

    #[test]
    fn test_parse_has_tool_calls_filter() {
        let expr = FilterParser::parse("~t").unwrap();
        assert!(matches!(expr, FilterExpr::Token(FilterToken::HasToolCalls)));
    }

    #[test]
    fn test_parse_has_thinking_filter() {
        let expr = FilterParser::parse("~k").unwrap();
        assert!(matches!(expr, FilterExpr::Token(FilterToken::HasThinking)));
    }

    #[test]
    fn test_parse_starred_filter() {
        let expr = FilterParser::parse("~starred").unwrap();
        assert!(matches!(expr, FilterExpr::Token(FilterToken::Starred)));
    }

    #[test]
    fn test_parse_tag_filter() {
        let expr = FilterParser::parse("~tag important").unwrap();
        assert!(matches!(expr, FilterExpr::Token(FilterToken::Tag(s)) if s == "important"));
    }

    #[test]
    fn test_parse_body_filter() {
        let expr = FilterParser::parse("~b hello").unwrap();
        assert!(matches!(expr, FilterExpr::Token(FilterToken::Body(s)) if s == "hello"));
    }

    #[test]
    fn test_parse_body_request_filter() {
        let expr = FilterParser::parse("~bq request").unwrap();
        assert!(matches!(expr, FilterExpr::Token(FilterToken::BodyRequest(s)) if s == "request"));
    }

    #[test]
    fn test_parse_body_response_filter() {
        let expr = FilterParser::parse("~bs response").unwrap();
        assert!(matches!(expr, FilterExpr::Token(FilterToken::BodyResponse(s)) if s == "response"));
    }

    #[test]
    fn test_parse_tokens_filter() {
        let expr = FilterParser::parse("~tokens >1000").unwrap();
        if let FilterExpr::Token(FilterToken::Tokens(c)) = expr {
            assert_eq!(c.op, ComparisonOp::Gt);
            assert_eq!(c.value, 1000);
        } else {
            panic!("Expected Tokens filter");
        }
    }

    #[test]
    fn test_parse_latency_filter_seconds() {
        let expr = FilterParser::parse("~latency >5s").unwrap();
        if let FilterExpr::Token(FilterToken::Latency(c)) = expr {
            assert_eq!(c.op, ComparisonOp::Gt);
            assert_eq!(c.value, 5000); // 5 seconds = 5000 ms
        } else {
            panic!("Expected Latency filter");
        }
    }

    #[test]
    fn test_parse_latency_filter_milliseconds() {
        let expr = FilterParser::parse("~latency >=500ms").unwrap();
        if let FilterExpr::Token(FilterToken::Latency(c)) = expr {
            assert_eq!(c.op, ComparisonOp::Gte);
            assert_eq!(c.value, 500);
        } else {
            panic!("Expected Latency filter");
        }
    }

    #[test]
    fn test_parse_and_expression() {
        let expr = FilterParser::parse("~p kiro & ~m claude").unwrap();
        assert!(matches!(expr, FilterExpr::And(_, _)));
    }

    #[test]
    fn test_parse_or_expression() {
        let expr = FilterParser::parse("~p kiro | ~p gemini").unwrap();
        assert!(matches!(expr, FilterExpr::Or(_, _)));
    }

    #[test]
    fn test_parse_not_expression() {
        let expr = FilterParser::parse("!~e").unwrap();
        assert!(matches!(expr, FilterExpr::Not(_)));
    }

    #[test]
    fn test_parse_grouped_expression() {
        let expr = FilterParser::parse("(~p kiro | ~p gemini) & ~m claude").unwrap();
        assert!(matches!(expr, FilterExpr::And(_, _)));
    }

    #[test]
    fn test_parse_complex_expression() {
        let expr = FilterParser::parse("~p kiro & ~m claude & !~e").unwrap();
        // Should parse as ((~p kiro & ~m claude) & !~e)
        assert!(matches!(expr, FilterExpr::And(_, _)));
    }

    #[test]
    fn test_parse_error_unknown_filter() {
        let result = FilterParser::parse("~unknown");
        assert!(matches!(result, Err(FilterParseError::UnknownFilter(_))));
    }

    #[test]
    fn test_parse_error_invalid_state() {
        let result = FilterParser::parse("~s invalid");
        assert!(matches!(result, Err(FilterParseError::InvalidState(_))));
    }

    #[test]
    fn test_parse_error_empty_expression() {
        let result = FilterParser::parse("");
        assert!(matches!(result, Err(FilterParseError::EmptyExpression)));
    }

    #[test]
    fn test_parse_error_unmatched_paren() {
        let result = FilterParser::parse("(~m claude");
        assert!(matches!(result, Err(FilterParseError::UnmatchedParen)));
    }

    #[test]
    fn test_evaluate_model_filter() {
        let flow = create_test_flow("claude-3-opus", ProviderType::Kiro);
        let expr = FilterParser::parse("~m claude").unwrap();
        let filter = FilterParser::compile(&expr);
        assert!(filter(&flow));

        let expr = FilterParser::parse("~m gpt").unwrap();
        let filter = FilterParser::compile(&expr);
        assert!(!filter(&flow));
    }

    #[test]
    fn test_evaluate_provider_filter() {
        let flow = create_test_flow("claude-3", ProviderType::Kiro);
        let expr = FilterParser::parse("~p kiro").unwrap();
        let filter = FilterParser::compile(&expr);
        assert!(filter(&flow));

        let expr = FilterParser::parse("~p openai").unwrap();
        let filter = FilterParser::compile(&expr);
        assert!(!filter(&flow));
    }

    #[test]
    fn test_evaluate_state_filter() {
        let mut flow = create_test_flow("claude-3", ProviderType::Kiro);
        flow.state = FlowState::Completed;

        let expr = FilterParser::parse("~s completed").unwrap();
        let filter = FilterParser::compile(&expr);
        assert!(filter(&flow));

        let expr = FilterParser::parse("~s pending").unwrap();
        let filter = FilterParser::compile(&expr);
        assert!(!filter(&flow));
    }

    #[test]
    fn test_evaluate_starred_filter() {
        let mut flow = create_test_flow("claude-3", ProviderType::Kiro);

        let expr = FilterParser::parse("~starred").unwrap();
        let filter = FilterParser::compile(&expr);
        assert!(!filter(&flow));

        flow.annotations.starred = true;
        assert!(filter(&flow));
    }

    #[test]
    fn test_evaluate_tag_filter() {
        let mut flow = create_test_flow("claude-3", ProviderType::Kiro);
        flow.annotations.tags = vec!["important".to_string(), "test".to_string()];

        let expr = FilterParser::parse("~tag important").unwrap();
        let filter = FilterParser::compile(&expr);
        assert!(filter(&flow));

        let expr = FilterParser::parse("~tag missing").unwrap();
        let filter = FilterParser::compile(&expr);
        assert!(!filter(&flow));
    }

    #[test]
    fn test_evaluate_tokens_filter() {
        let mut flow = create_test_flow("claude-3", ProviderType::Kiro);
        flow.response = Some(LLMResponse {
            usage: TokenUsage {
                input_tokens: 500,
                output_tokens: 600,
                total_tokens: 1100,
                ..Default::default()
            },
            ..Default::default()
        });

        let expr = FilterParser::parse("~tokens >1000").unwrap();
        let filter = FilterParser::compile(&expr);
        assert!(filter(&flow));

        let expr = FilterParser::parse("~tokens <1000").unwrap();
        let filter = FilterParser::compile(&expr);
        assert!(!filter(&flow));
    }

    #[test]
    fn test_evaluate_latency_filter() {
        let mut flow = create_test_flow("claude-3", ProviderType::Kiro);
        flow.timestamps.duration_ms = 6000; // 6 seconds

        let expr = FilterParser::parse("~latency >5s").unwrap();
        let filter = FilterParser::compile(&expr);
        assert!(filter(&flow));

        let expr = FilterParser::parse("~latency <5000ms").unwrap();
        let filter = FilterParser::compile(&expr);
        assert!(!filter(&flow));
    }

    #[test]
    fn test_evaluate_and_expression() {
        let flow = create_test_flow("claude-3-opus", ProviderType::Kiro);

        let expr = FilterParser::parse("~p kiro & ~m claude").unwrap();
        let filter = FilterParser::compile(&expr);
        assert!(filter(&flow));

        let expr = FilterParser::parse("~p openai & ~m claude").unwrap();
        let filter = FilterParser::compile(&expr);
        assert!(!filter(&flow));
    }

    #[test]
    fn test_evaluate_or_expression() {
        let flow = create_test_flow("claude-3", ProviderType::Kiro);

        let expr = FilterParser::parse("~p kiro | ~p gemini").unwrap();
        let filter = FilterParser::compile(&expr);
        assert!(filter(&flow));

        let expr = FilterParser::parse("~p openai | ~p gemini").unwrap();
        let filter = FilterParser::compile(&expr);
        assert!(!filter(&flow));
    }

    #[test]
    fn test_evaluate_not_expression() {
        let flow = create_test_flow("claude-3", ProviderType::Kiro);

        let expr = FilterParser::parse("!~e").unwrap();
        let filter = FilterParser::compile(&expr);
        assert!(filter(&flow)); // No error, so !~e is true
    }

    #[test]
    fn test_display_filter_expr() {
        let expr = FilterParser::parse("~p kiro & ~m claude").unwrap();
        let display = format!("{}", expr);
        assert!(display.contains("~p kiro"));
        assert!(display.contains("~m claude"));
    }

    #[test]
    fn test_round_trip_simple() {
        let original = "~m claude";
        let expr = FilterParser::parse(original).unwrap();
        let display = format!("{}", expr);
        let reparsed = FilterParser::parse(&display).unwrap();
        assert_eq!(format!("{}", expr), format!("{}", reparsed));
    }
}

// ============================================================================
// 属性测试
// ============================================================================

#[cfg(test)]
mod property_tests {
    use super::*;
    use crate::flow_monitor::models::{
        FlowAnnotations, FlowError, FlowErrorType, FlowMetadata, FlowTimestamps, FlowType,
        FunctionCall, LLMRequest, LLMResponse, Message, MessageContent, MessageRole,
        RequestParameters, ThinkingContent, TokenUsage, ToolCall,
    };
    use crate::ProviderType;
    use proptest::prelude::*;

    // ========================================================================
    // 生成器
    // ========================================================================

    /// 生成随机的 ProviderType
    fn arb_provider_type() -> impl Strategy<Value = ProviderType> {
        prop_oneof![
            Just(ProviderType::Kiro),
            Just(ProviderType::Gemini),
            Just(ProviderType::Qwen),
            Just(ProviderType::OpenAI),
            Just(ProviderType::Claude),
            Just(ProviderType::Antigravity),
        ]
    }

    /// 生成随机的 FlowState
    fn arb_flow_state() -> impl Strategy<Value = FlowState> {
        prop_oneof![
            Just(FlowState::Pending),
            Just(FlowState::Streaming),
            Just(FlowState::Completed),
            Just(FlowState::Failed),
            Just(FlowState::Cancelled),
        ]
    }

    /// 生成随机的模型名称
    fn arb_model_name() -> impl Strategy<Value = String> {
        prop_oneof![
            Just("gpt-4".to_string()),
            Just("gpt-4-turbo".to_string()),
            Just("gpt-3.5-turbo".to_string()),
            Just("claude-3-opus".to_string()),
            Just("claude-3-sonnet".to_string()),
            Just("gemini-pro".to_string()),
            Just("qwen-max".to_string()),
        ]
    }

    /// 生成随机的标签
    fn arb_tags() -> impl Strategy<Value = Vec<String>> {
        prop::collection::vec("[a-z]{3,10}", 0..5)
    }

    /// 生成随机的 LLMFlow
    fn arb_llm_flow() -> impl Strategy<Value = LLMFlow> {
        (
            "[a-f0-9]{8}",
            arb_model_name(),
            arb_provider_type(),
            arb_flow_state(),
            any::<bool>(),  // starred
            arb_tags(),     // tags
            any::<bool>(),  // has_error
            any::<bool>(),  // has_tool_calls
            any::<bool>(),  // has_thinking
            0u32..50000u32, // total_tokens
            0u64..30000u64, // duration_ms
        )
            .prop_map(
                |(
                    id,
                    model,
                    provider,
                    state,
                    starred,
                    tags,
                    has_error,
                    has_tool_calls,
                    has_thinking,
                    total_tokens,
                    duration_ms,
                )| {
                    let request = LLMRequest {
                        method: "POST".to_string(),
                        path: "/v1/chat/completions".to_string(),
                        model,
                        parameters: RequestParameters::default(),
                        ..Default::default()
                    };

                    let metadata = FlowMetadata {
                        provider,
                        ..Default::default()
                    };

                    let mut flow = LLMFlow::new(id, FlowType::ChatCompletions, request, metadata);
                    flow.state = state;
                    flow.annotations.starred = starred;
                    flow.annotations.tags = tags;
                    flow.timestamps.duration_ms = duration_ms;

                    // 设置错误
                    if has_error {
                        flow.error = Some(FlowError::new(FlowErrorType::ServerError, "Test error"));
                    }

                    // 设置响应
                    let mut response = LLMResponse {
                        usage: TokenUsage {
                            input_tokens: total_tokens / 2,
                            output_tokens: total_tokens / 2,
                            total_tokens,
                            ..Default::default()
                        },
                        ..Default::default()
                    };

                    // 设置工具调用
                    if has_tool_calls {
                        response.tool_calls = vec![ToolCall {
                            id: "call_1".to_string(),
                            tool_type: "function".to_string(),
                            function: FunctionCall {
                                name: "test_function".to_string(),
                                arguments: "{}".to_string(),
                            },
                        }];
                    }

                    // 设置思维链
                    if has_thinking {
                        response.thinking = Some(ThinkingContent {
                            text: "Thinking...".to_string(),
                            tokens: Some(100),
                            signature: None,
                        });
                    }

                    flow.response = Some(response);
                    flow
                },
            )
    }

    // ========================================================================
    // Property 1: 过滤表达式正确性
    // ========================================================================

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        /// **Feature: flow-monitor-enhancement, Property 1: 过滤表达式正确性**
        /// **Validates: Requirements 1.1-1.16**
        ///
        /// *对于任意* 有效的过滤表达式和 Flow 集合，解析并执行过滤后，
        /// 返回的所有 Flow 都应该满足该表达式定义的条件。
        #[test]
        fn prop_filter_model_correctness(
            flow in arb_llm_flow(),
        ) {
            // 测试模型过滤器正确性
            let model = flow.request.model.clone();
            let expr_str = format!("~m {}", model);
            let expr = FilterParser::parse(&expr_str).unwrap();
            let filter = FilterParser::compile(&expr);

            // 使用完整模型名称过滤应该匹配
            prop_assert!(
                filter(&flow),
                "模型过滤器 '{}' 应该匹配模型 '{}'",
                expr_str,
                model
            );
        }

        #[test]
        fn prop_filter_provider_correctness(
            flow in arb_llm_flow(),
        ) {
            // 测试提供商过滤器正确性
            let provider_str = format!("{:?}", flow.metadata.provider).to_lowercase();
            let expr_str = format!("~p {}", provider_str);
            let expr = FilterParser::parse(&expr_str).unwrap();
            let filter = FilterParser::compile(&expr);

            prop_assert!(
                filter(&flow),
                "提供商过滤器 '{}' 应该匹配提供商 '{:?}'",
                expr_str,
                flow.metadata.provider
            );
        }

        #[test]
        fn prop_filter_state_correctness(
            flow in arb_llm_flow(),
        ) {
            // 测试状态过滤器正确性
            let state_str = state_to_string(&flow.state);
            let expr_str = format!("~s {}", state_str);
            let expr = FilterParser::parse(&expr_str).unwrap();
            let filter = FilterParser::compile(&expr);

            prop_assert!(
                filter(&flow),
                "状态过滤器 '{}' 应该匹配状态 '{:?}'",
                expr_str,
                flow.state
            );
        }

        #[test]
        fn prop_filter_error_correctness(
            flow in arb_llm_flow(),
        ) {
            // 测试错误过滤器正确性
            let expr = FilterParser::parse("~e").unwrap();
            let filter = FilterParser::compile(&expr);
            let result = filter(&flow);

            prop_assert_eq!(
                result,
                flow.error.is_some(),
                "错误过滤器结果应该与 flow.error.is_some() 一致"
            );
        }

        #[test]
        fn prop_filter_tool_calls_correctness(
            flow in arb_llm_flow(),
        ) {
            // 测试工具调用过滤器正确性
            let expr = FilterParser::parse("~t").unwrap();
            let filter = FilterParser::compile(&expr);
            let result = filter(&flow);

            let has_tool_calls = flow
                .response
                .as_ref()
                .map_or(false, |r| !r.tool_calls.is_empty());

            prop_assert_eq!(
                result,
                has_tool_calls,
                "工具调用过滤器结果应该与实际工具调用状态一致"
            );
        }

        #[test]
        fn prop_filter_thinking_correctness(
            flow in arb_llm_flow(),
        ) {
            // 测试思维链过滤器正确性
            let expr = FilterParser::parse("~k").unwrap();
            let filter = FilterParser::compile(&expr);
            let result = filter(&flow);

            let has_thinking = flow
                .response
                .as_ref()
                .map_or(false, |r| r.thinking.is_some());

            prop_assert_eq!(
                result,
                has_thinking,
                "思维链过滤器结果应该与实际思维链状态一致"
            );
        }

        #[test]
        fn prop_filter_starred_correctness(
            flow in arb_llm_flow(),
        ) {
            // 测试收藏过滤器正确性
            let expr = FilterParser::parse("~starred").unwrap();
            let filter = FilterParser::compile(&expr);
            let result = filter(&flow);

            prop_assert_eq!(
                result,
                flow.annotations.starred,
                "收藏过滤器结果应该与 flow.annotations.starred 一致"
            );
        }

        #[test]
        fn prop_filter_tokens_correctness(
            flow in arb_llm_flow(),
            threshold in 0i64..50000i64,
        ) {
            // 测试 Token 数量过滤器正确性
            let total_tokens = flow
                .response
                .as_ref()
                .map_or(0, |r| r.usage.total_tokens as i64);

            // 测试大于
            let expr_str = format!("~tokens >{}", threshold);
            let expr = FilterParser::parse(&expr_str).unwrap();
            let filter = FilterParser::compile(&expr);
            let result = filter(&flow);

            prop_assert_eq!(
                result,
                total_tokens > threshold,
                "Token 过滤器 '{}' 结果应该正确 (actual: {}, threshold: {})",
                expr_str,
                total_tokens,
                threshold
            );
        }

        #[test]
        fn prop_filter_latency_correctness(
            flow in arb_llm_flow(),
            threshold in 0i64..30000i64,
        ) {
            // 测试延迟过滤器正确性
            let duration_ms = flow.timestamps.duration_ms as i64;

            // 测试大于
            let expr_str = format!("~latency >{}ms", threshold);
            let expr = FilterParser::parse(&expr_str).unwrap();
            let filter = FilterParser::compile(&expr);
            let result = filter(&flow);

            prop_assert_eq!(
                result,
                duration_ms > threshold,
                "延迟过滤器 '{}' 结果应该正确 (actual: {}, threshold: {})",
                expr_str,
                duration_ms,
                threshold
            );
        }

        #[test]
        fn prop_filter_and_correctness(
            flow in arb_llm_flow(),
        ) {
            // 测试 AND 逻辑正确性
            let model = flow.request.model.clone();
            let provider_str = format!("{:?}", flow.metadata.provider).to_lowercase();

            let expr_str = format!("~m {} & ~p {}", model, provider_str);
            let expr = FilterParser::parse(&expr_str).unwrap();
            let filter = FilterParser::compile(&expr);

            // 两个条件都应该满足
            prop_assert!(
                filter(&flow),
                "AND 表达式 '{}' 应该匹配",
                expr_str
            );
        }

        #[test]
        fn prop_filter_or_correctness(
            flow in arb_llm_flow(),
        ) {
            // 测试 OR 逻辑正确性
            let model = flow.request.model.clone();

            // 使用一个匹配的条件和一个不匹配的条件
            let expr_str = format!("~m {} | ~m nonexistent-model-xyz", model);
            let expr = FilterParser::parse(&expr_str).unwrap();
            let filter = FilterParser::compile(&expr);

            // 至少一个条件满足
            prop_assert!(
                filter(&flow),
                "OR 表达式 '{}' 应该匹配",
                expr_str
            );
        }

        #[test]
        fn prop_filter_not_correctness(
            flow in arb_llm_flow(),
        ) {
            // 测试 NOT 逻辑正确性
            let expr = FilterParser::parse("~e").unwrap();
            let filter_e = FilterParser::compile(&expr);
            let result_e = filter_e(&flow);

            let expr_not = FilterParser::parse("!~e").unwrap();
            let filter_not_e = FilterParser::compile(&expr_not);
            let result_not_e = filter_not_e(&flow);

            prop_assert_eq!(
                result_not_e,
                !result_e,
                "NOT 表达式结果应该是原表达式的取反"
            );
        }
    }

    // ========================================================================
    // Property 2: 过滤表达式 Round-Trip
    // ========================================================================

    /// 生成随机的比较运算符
    fn arb_comparison_op() -> impl Strategy<Value = ComparisonOp> {
        prop_oneof![
            Just(ComparisonOp::Gt),
            Just(ComparisonOp::Gte),
            Just(ComparisonOp::Lt),
            Just(ComparisonOp::Lte),
            Just(ComparisonOp::Eq),
        ]
    }

    /// 生成随机的 Comparison
    fn arb_comparison() -> impl Strategy<Value = Comparison> {
        (arb_comparison_op(), 0i64..100000i64).prop_map(|(op, value)| Comparison { op, value })
    }

    /// 生成随机的简单 FilterToken（不包括逻辑运算符和括号）
    fn arb_simple_filter_token() -> impl Strategy<Value = FilterToken> {
        prop_oneof![
            arb_model_name().prop_map(FilterToken::Model),
            prop_oneof![
                Just("kiro".to_string()),
                Just("openai".to_string()),
                Just("claude".to_string()),
                Just("gemini".to_string()),
            ]
            .prop_map(FilterToken::Provider),
            arb_flow_state().prop_map(FilterToken::State),
            Just(FilterToken::HasError),
            Just(FilterToken::HasToolCalls),
            Just(FilterToken::HasThinking),
            Just(FilterToken::Starred),
            "[a-z]{3,8}".prop_map(FilterToken::Tag),
            arb_comparison().prop_map(FilterToken::Tokens),
            arb_comparison().prop_map(FilterToken::Latency),
        ]
    }

    /// 生成随机的 FilterExpr
    fn arb_filter_expr() -> impl Strategy<Value = FilterExpr> {
        arb_simple_filter_token()
            .prop_map(FilterExpr::Token)
            .prop_recursive(3, 10, 5, |inner| {
                prop_oneof![
                    inner.clone().prop_map(|e| FilterExpr::Not(Box::new(e))),
                    (inner.clone(), inner.clone())
                        .prop_map(|(l, r)| FilterExpr::And(Box::new(l), Box::new(r))),
                    (inner.clone(), inner)
                        .prop_map(|(l, r)| FilterExpr::Or(Box::new(l), Box::new(r))),
                ]
            })
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        /// **Feature: flow-monitor-enhancement, Property 2: 过滤表达式 Round-Trip**
        /// **Validates: Requirements 1.1-1.16**
        ///
        /// *对于任意* 有效的过滤表达式，解析为 AST 后再序列化回字符串，
        /// 重新解析应该产生语义等价的 AST（对同一 Flow 产生相同的过滤结果）。
        #[test]
        fn prop_filter_expr_round_trip(
            expr in arb_filter_expr(),
            flow in arb_llm_flow(),
        ) {
            // 序列化为字符串
            let expr_str = format!("{}", expr);

            // 重新解析
            let reparsed = FilterParser::parse(&expr_str);
            prop_assert!(
                reparsed.is_ok(),
                "序列化后的表达式 '{}' 应该能够重新解析",
                expr_str
            );

            let reparsed_expr = reparsed.unwrap();

            // 编译两个表达式
            let filter1 = FilterParser::compile(&expr);
            let filter2 = FilterParser::compile(&reparsed_expr);

            // 对同一 Flow 应该产生相同的结果
            let result1 = filter1(&flow);
            let result2 = filter2(&flow);

            prop_assert_eq!(
                result1,
                result2,
                "原始表达式和重新解析的表达式对同一 Flow 应该产生相同的结果\n原始: {}\n重新解析: {}",
                format!("{}", expr),
                format!("{}", reparsed_expr)
            );
        }

        /// 测试简单表达式的 Round-Trip
        #[test]
        fn prop_simple_filter_round_trip(
            token in arb_simple_filter_token(),
            flow in arb_llm_flow(),
        ) {
            let expr = FilterExpr::Token(token);
            let expr_str = format!("{}", expr);

            // 重新解析
            let reparsed = FilterParser::parse(&expr_str);
            prop_assert!(
                reparsed.is_ok(),
                "简单表达式 '{}' 应该能够重新解析",
                expr_str
            );

            let reparsed_expr = reparsed.unwrap();

            // 编译并比较结果
            let filter1 = FilterParser::compile(&expr);
            let filter2 = FilterParser::compile(&reparsed_expr);

            prop_assert_eq!(
                filter1(&flow),
                filter2(&flow),
                "简单表达式 Round-Trip 应该保持语义一致"
            );
        }
    }

    // ========================================================================
    // Property 3: 过滤表达式错误处理
    // ========================================================================

    /// 生成无效的过滤器名称
    fn arb_invalid_filter_name() -> impl Strategy<Value = String> {
        prop_oneof![
            Just("unknown".to_string()),
            Just("invalid".to_string()),
            Just("xyz".to_string()),
            Just("foo".to_string()),
            Just("bar".to_string()),
            "[a-z]{5,10}".prop_filter("Filter out valid names", |s| {
                ![
                    "m", "p", "s", "e", "t", "k", "b", "bq", "bs", "starred", "tag", "tokens",
                    "latency",
                ]
                .contains(&s.as_str())
            }),
        ]
    }

    /// 生成无效的状态值
    fn arb_invalid_state() -> impl Strategy<Value = String> {
        prop_oneof![
            Just("invalid".to_string()),
            Just("unknown".to_string()),
            Just("running".to_string()),
            Just("stopped".to_string()),
            "[a-z]{5,10}".prop_filter("Filter out valid states", |s| {
                !["pending", "streaming", "completed", "failed", "cancelled"]
                    .contains(&s.to_lowercase().as_str())
            }),
        ]
    }

    /// 生成无效的比较运算符
    fn arb_invalid_comparison_op() -> impl Strategy<Value = String> {
        prop_oneof![
            Just("==".to_string()),
            Just("!=".to_string()),
            Just("<>".to_string()),
            Just("~".to_string()),
            Just("@".to_string()),
        ]
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        /// **Feature: flow-monitor-enhancement, Property 3: 过滤表达式错误处理**
        /// **Validates: Requirements 1.17**
        ///
        /// *对于任意* 无效的过滤表达式，解析器应该返回错误而不是 panic，
        /// 且错误信息应该包含有用的诊断信息。
        #[test]
        fn prop_invalid_filter_returns_error(
            filter_name in arb_invalid_filter_name(),
        ) {
            let expr_str = format!("~{}", filter_name);
            let result = FilterParser::parse(&expr_str);

            // 应该返回错误
            prop_assert!(
                result.is_err(),
                "无效的过滤器 '{}' 应该返回错误",
                expr_str
            );

            // 错误应该是 UnknownFilter
            if let Err(e) = result {
                prop_assert!(
                    matches!(e, FilterParseError::UnknownFilter(_)),
                    "错误类型应该是 UnknownFilter，实际是: {:?}",
                    e
                );
            }
        }

        #[test]
        fn prop_invalid_state_returns_error(
            state in arb_invalid_state(),
        ) {
            let expr_str = format!("~s {}", state);
            let result = FilterParser::parse(&expr_str);

            // 应该返回错误
            prop_assert!(
                result.is_err(),
                "无效的状态 '{}' 应该返回错误",
                expr_str
            );

            // 错误应该是 InvalidState
            if let Err(e) = result {
                prop_assert!(
                    matches!(e, FilterParseError::InvalidState(_)),
                    "错误类型应该是 InvalidState，实际是: {:?}",
                    e
                );
            }
        }

        #[test]
        fn prop_invalid_comparison_op_returns_error(
            op in arb_invalid_comparison_op(),
        ) {
            let expr_str = format!("~tokens {}100", op);
            let result = FilterParser::parse(&expr_str);

            // 应该返回错误
            prop_assert!(
                result.is_err(),
                "无效的比较运算符 '{}' 应该返回错误",
                expr_str
            );
        }

        #[test]
        fn prop_unmatched_paren_returns_error(
            depth in 1usize..5usize,
        ) {
            // 生成不匹配的括号
            let open_parens: String = (0..depth).map(|_| '(').collect();
            let expr_str = format!("{}~e", open_parens);
            let result = FilterParser::parse(&expr_str);

            // 应该返回错误
            prop_assert!(
                result.is_err(),
                "不匹配的括号 '{}' 应该返回错误",
                expr_str
            );
        }

        #[test]
        fn prop_empty_expression_returns_error(
            spaces in " {0,10}",
        ) {
            let result = FilterParser::parse(&spaces);

            // 应该返回错误
            prop_assert!(
                result.is_err(),
                "空表达式应该返回错误"
            );

            // 错误应该是 EmptyExpression
            if let Err(e) = result {
                prop_assert!(
                    matches!(e, FilterParseError::EmptyExpression),
                    "错误类型应该是 EmptyExpression，实际是: {:?}",
                    e
                );
            }
        }

        #[test]
        fn prop_missing_argument_returns_error(
            filter in prop_oneof![
                Just("m"),
                Just("p"),
                Just("s"),
                Just("tag"),
                Just("b"),
                Just("bq"),
                Just("bs"),
            ],
        ) {
            // 缺少参数的过滤器
            let expr_str = format!("~{}", filter);
            let result = FilterParser::parse(&expr_str);

            // 应该返回错误（缺少参数）
            prop_assert!(
                result.is_err(),
                "缺少参数的过滤器 '{}' 应该返回错误",
                expr_str
            );
        }

        /// 测试解析器不会 panic
        #[test]
        fn prop_parser_never_panics(
            input in "[ -~]{0,50}",
        ) {
            // 尝试解析任意输入，不应该 panic
            let _ = FilterParser::parse(&input);
            // 如果没有 panic，测试通过
        }
    }
}
