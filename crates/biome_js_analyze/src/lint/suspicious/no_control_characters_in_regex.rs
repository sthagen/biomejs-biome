use biome_analyze::{
    Ast, Rule, RuleDiagnostic, RuleSource, context::RuleContext, declare_lint_rule,
};
use biome_console::markup;
use biome_diagnostics::Severity;
use biome_js_syntax::JsRegexLiteralExpression;
use biome_rowan::{AstNode, TextRange, TextSize};
use biome_rule_options::no_control_characters_in_regex::NoControlCharactersInRegexOptions;
use core::str;

declare_lint_rule! {
    /// Prevents from having control characters and some escape sequences that match control characters in regular expression literals.
    ///
    /// Control characters are hidden special characters that are numbered from 0 to 31 in the ASCII system.
    /// They're not commonly used in JavaScript text. So, if you see them in a pattern (called a regular expression), it's probably a mistake.
    ///
    /// The following elements of regular expression patterns are considered possible errors in typing and are therefore disallowed by this rule:
    ///
    /// - Hexadecimal character escapes from `\x00` to `\x1F`
    /// - Unicode character escapes from `\u0000` to `\u001F`
    /// - Unicode code point escapes from `\u{0}` to `\u{1F}`
    /// - Unescaped raw characters from U+0000 to U+001F
    ///
    /// Control escapes such as `\t` and `\n` are allowed by this rule.
    ///
    /// ## Examples
    ///
    /// ### Invalid
    /// ```js,expect_diagnostic
    ///  var pattern1 = /\x00/;
    /// ```
    /// ```js,expect_diagnostic
    ///  var pattern2 = /\x0C/;
    /// ```
    /// ```js,expect_diagnostic
    ///  var pattern3 = /\x1F/;
    /// ```
    /// ```js,expect_diagnostic
    ///  var pattern4 = /\u000C/;
    /// ```
    /// ```js,expect_diagnostic
    ///  var pattern5 = /\u{C}/u;
    /// ```
    ///
    /// ### Valid
    /// ```js
    /// var pattern1 = /\x20/;
    /// var pattern2 = /\u0020/;
    /// var pattern3 = /\u{20}/u;
    /// var pattern4 = /\t/;
    /// var pattern5 = /\n/;
    /// ```
    ///
    pub NoControlCharactersInRegex {
        version: "1.0.0",
        name: "noControlCharactersInRegex",
        language: "js",
        sources: &[RuleSource::Eslint("no-control-regex").same()],
        recommended: true,
        severity: Severity::Error,
    }
}

fn decode_hex(digits: &[u8]) -> Option<u32> {
    str::from_utf8(digits)
        .ok()
        .and_then(|digits| u32::from_str_radix(digits, 16).ok())
}

/// Collecting control characters for regex. The following characters in regular expression patterns are considered as control characters:
/// - Hexadecimal character escapes from `\x00` to `\x1F`.
/// - Unicode character escapes from `\u0000` to `\u001F`.
/// - Unicode code point escapes range from `\u{0}` to `\u{1F}`.
///     - The Unicode flag must be set as true in order for these Unicode code point escapes to work: https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/RegExp/unicode.
/// - Unescaped raw characters from U+0000 to U+001F.
fn collect_control_characters(
    pattern_index: TextSize,
    pattern: &str,
    flags: &str,
    is_pattern_in_str: bool,
) -> Option<Vec<TextRange>> {
    let mut control_chars = Vec::new();
    let is_unicode_flag_set = flags.contains('u') || flags.contains('v');
    let bytes = pattern.as_bytes();
    let mut iter = pattern.bytes().enumerate();

    while let Some((index, c)) = iter.next() {
        let decoded = match c {
            b'\\' => {
                let Some((escaped_index, c)) = iter.next() else {
                    break;
                };
                let (is_str_escape_seq, escaped_index, c) = if c == b'\\' && is_pattern_in_str {
                    let Some((escaped_index, c)) = iter.next() else {
                        break;
                    };
                    (false, escaped_index, c)
                } else {
                    (is_pattern_in_str, escaped_index, c)
                };
                let hex_index = escaped_index + 1;
                match c {
                    b'x' if (hex_index + 2) <= bytes.len() => (
                        decode_hex(&bytes[hex_index..(hex_index + 2)]),
                        hex_index + 2,
                    ),
                    b'u' if is_str_escape_seq || is_unicode_flag_set => {
                        if matches!(iter.next(), Some((_, b'{'))) {
                            let hex_index = hex_index + 1;
                            let Some((end, _)) = iter.find(|(_, c)| c == &b'}') else {
                                continue;
                            };
                            (decode_hex(&bytes[hex_index..end]), end + 1)
                        } else if (hex_index + 4) <= bytes.len() {
                            (
                                decode_hex(&bytes[hex_index..(hex_index + 4)]),
                                hex_index + 4,
                            )
                        } else {
                            continue;
                        }
                    }
                    b'u' if (hex_index + 4) <= bytes.len() => (
                        decode_hex(&bytes[hex_index..(hex_index + 4)]),
                        hex_index + 4,
                    ),
                    // Control character in code source
                    0..=31 => (Some(c as u32), escaped_index + 1),
                    _ => {
                        continue;
                    }
                }
            }
            // Control character in code source
            0..=31 => (Some(c as u32), index + 1),
            _ => {
                continue;
            }
        };
        let (Some(control_char), end) = decoded else {
            continue;
        };
        if matches!(control_char, 0..=31) {
            let range = TextRange::new(
                pattern_index + TextSize::from(index as u32),
                pattern_index + TextSize::from(end as u32),
            );
            control_chars.push(range);
        }
    }
    Some(control_chars)
}

impl Rule for NoControlCharactersInRegex {
    type Query = Ast<JsRegexLiteralExpression>;
    type State = TextRange;
    type Signals = Box<[Self::State]>;
    type Options = NoControlCharactersInRegexOptions;

    fn run(ctx: &RuleContext<Self>) -> Self::Signals {
        let node = ctx.query();
        let Ok((pattern, flags)) = node.decompose() else {
            return Default::default();
        };
        let pattern_start = node.range().start() + TextSize::from(1);
        collect_control_characters(pattern_start, pattern.text(), flags.text(), false)
            .unwrap_or_default()
            .into_boxed_slice()
    }

    fn diagnostic(_: &RuleContext<Self>, state: &Self::State) -> Option<RuleDiagnostic> {
        Some(RuleDiagnostic::new(
            rule_category!(),
            state,
            markup! {
                "Unexpected control character in a regular expression."
            },
        ).note(
            markup! {
                "Control characters are unusual and potentially incorrect inputs, so they are disallowed."
            }
        ))
    }
}
