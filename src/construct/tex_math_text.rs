//! Source-faithful `\(…\)` math in text. See `ZEVRIA.md`.
//!
//! Backslashes pair with the next byte, including another backslash. Failed
//! scans are memoized per text tokenizer, not per parse or source document.

use crate::event::Name;
use crate::state::{Name as StateName, State};
use crate::tokenizer::Tokenizer;

/// Try TeX math before Markdown's character escape.
pub fn start(tokenizer: &mut Tokenizer) -> State {
    let index = tokenizer.point.index;
    if !tokenizer.parse_state.options.constructs.math_text
        || tokenizer.current != Some(b'\\')
        || tokenizer.parse_state.bytes.get(index + 1) != Some(&b'(')
        || tokenizer
            .tokenize_state
            .tex_math_text_failed
            .map_or(false, |failed| index >= failed)
    {
        return State::Nok;
    }
    tokenizer.tokenize_state.tex_math_text_start = index;
    tokenizer.enter(Name::MathText);
    tokenizer.enter(Name::MathTextSequence);
    tokenizer.consume();
    State::Next(StateName::TexMathTextOpen)
}

/// Consume the second opener byte.
pub fn open(tokenizer: &mut Tokenizer) -> State {
    debug_assert_eq!(tokenizer.current, Some(b'('));
    tokenizer.consume();
    tokenizer.exit(Name::MathTextSequence);
    State::Next(StateName::TexMathTextBetween)
}

/// Start a data chunk or line ending, or finish at the first unpaired `\)`.
pub fn between(tokenizer: &mut Tokenizer) -> State {
    match tokenizer.current {
        None => {
            tokenizer.tokenize_state.tex_math_text_failed =
                Some(tokenizer.tokenize_state.tex_math_text_start);
            State::Nok
        }
        Some(b'\n') => {
            tokenizer.enter(Name::LineEnding);
            tokenizer.consume();
            tokenizer.exit(Name::LineEnding);
            State::Next(StateName::TexMathTextBetween)
        }
        Some(b'\\')
            if tokenizer.parse_state.bytes.get(tokenizer.point.index + 1) == Some(&b')') =>
        {
            tokenizer.enter(Name::MathTextSequence);
            tokenizer.consume();
            State::Next(StateName::TexMathTextClose)
        }
        _ => {
            tokenizer.enter(Name::MathTextData);
            State::Retry(StateName::TexMathTextData)
        }
    }
}

/// Consume raw TeX without letting Markdown parse its punctuation.
pub fn data(tokenizer: &mut Tokenizer) -> State {
    match tokenizer.current {
        None | Some(b'\n') => {
            tokenizer.exit(Name::MathTextData);
            State::Retry(StateName::TexMathTextBetween)
        }
        Some(b'\\') => {
            // Close before the delimiter, but keep paired backslashes in data.
            if tokenizer.parse_state.bytes.get(tokenizer.point.index + 1) == Some(&b')') {
                tokenizer.exit(Name::MathTextData);
                State::Retry(StateName::TexMathTextBetween)
            } else {
                tokenizer.consume();
                State::Next(StateName::TexMathTextEscaped)
            }
        }
        _ => {
            tokenizer.consume();
            State::Next(StateName::TexMathTextData)
        }
    }
}

/// A backslash pairs with the next byte, so `\\)` cannot close math.
pub fn escaped(tokenizer: &mut Tokenizer) -> State {
    if matches!(tokenizer.current, None | Some(b'\n')) {
        State::Retry(StateName::TexMathTextData)
    } else {
        tokenizer.consume();
        State::Next(StateName::TexMathTextData)
    }
}

/// Consume the second closer byte.
pub fn close(tokenizer: &mut Tokenizer) -> State {
    debug_assert_eq!(tokenizer.current, Some(b')'));
    tokenizer.consume();
    tokenizer.exit(Name::MathTextSequence);
    tokenizer.exit(Name::MathText);
    State::Ok
}
