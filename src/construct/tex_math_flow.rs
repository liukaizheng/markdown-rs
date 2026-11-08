//! Line-start TeX displays. Uses the existing math events and compilers.
//!
//! Recognition is decided on the first line: `\[` alone is a fence; `\[…\]`
//! and `$$…$$` must occupy one whole line. Raw flow is tried first, preserving
//! dollar fences (including `$$$`). Backslash pairs are opaque TeX data.

use crate::construct::partial_space_or_tab::space_or_tab_min_max;
use crate::event::Name;
use crate::state::{Name as StateName, State};
use crate::tokenizer::Tokenizer;
use crate::util::slice::{Position, Slice};

/// Allow at most three spaces, just like a fenced code opener.
pub fn start(tokenizer: &mut Tokenizer) -> State {
    if !tokenizer.parse_state.options.constructs.math_flow {
        return State::Nok;
    }
    if matches!(tokenizer.current, Some(b' ' | b'\t')) {
        tokenizer.attempt(State::Next(StateName::TexMathFlowOpen), State::Nok);
        State::Retry(space_or_tab_min_max(tokenizer, 0, 3))
    } else {
        State::Retry(StateName::TexMathFlowOpen)
    }
}

/// Decide between the fence and single-line forms without consuming content.
pub fn open(tokenizer: &mut Tokenizer) -> State {
    let bytes = tokenizer.parse_state.bytes;
    let index = tokenizer.point.index;
    let marker = match bytes.get(index..index + 2) {
        Some(b"\\[") => b'\\',
        Some(b"$$") if bytes.get(index + 2) != Some(&b'$') => b'$',
        _ => return State::Nok,
    };
    let mut end = index + 2;
    while end < bytes.len() && !matches!(bytes[end], b'\r' | b'\n') {
        end += 1;
    }
    let fenced = marker == b'\\'
        && bytes[index + 2..end]
            .iter()
            .all(|b| matches!(b, b' ' | b'\t'));
    if !fenced {
        let mut cursor = index + 2;
        let mut closed = false;
        while cursor + 1 < end {
            if (marker == b'\\' && &bytes[cursor..cursor + 2] == b"\\]")
                || (marker == b'$' && &bytes[cursor..cursor + 2] == b"$$")
            {
                closed = bytes[cursor + 2..end]
                    .iter()
                    .all(|b| matches!(b, b' ' | b'\t'));
                break;
            }
            cursor += if bytes[cursor] == b'\\' { 2 } else { 1 };
        }
        if !closed {
            return State::Nok;
        }
    }
    let prefix = tokenizer.events.last().map_or(0, |event| {
        if event.name == Name::SpaceOrTab {
            Slice::from_position(
                bytes,
                &Position::from_exit_event(&tokenizer.events, tokenizer.events.len() - 1),
            )
            .len()
        } else {
            0
        }
    });
    tokenizer.tokenize_state.tex_math_flow_marker = marker;
    tokenizer.tokenize_state.tex_math_flow_indent = prefix;
    tokenizer.tokenize_state.tex_math_flow_fenced = fenced;
    tokenizer.enter(Name::MathFlow);
    tokenizer.enter(Name::MathFlowFence);
    tokenizer.enter(Name::MathFlowFenceSequence);
    tokenizer.consume();
    State::Next(StateName::TexMathFlowOpenSecond)
}

/// Finish the opening delimiter.
pub fn open_second(tokenizer: &mut Tokenizer) -> State {
    tokenizer.consume();
    tokenizer.exit(Name::MathFlowFenceSequence);
    if tokenizer.tokenize_state.tex_math_flow_fenced {
        State::Next(StateName::TexMathFlowOpenTrailing)
    } else {
        tokenizer.exit(Name::MathFlowFence);
        tokenizer.concrete = true;
        State::Next(StateName::TexMathFlowBeforeChunk)
    }
}

/// A bare bracket opener may have trailing horizontal whitespace.
pub fn open_trailing(tokenizer: &mut Tokenizer) -> State {
    if matches!(tokenizer.current, Some(b' ' | b'\t')) {
        tokenizer.consume();
        State::Next(StateName::TexMathFlowOpenTrailing)
    } else {
        tokenizer.exit(Name::MathFlowFence);
        tokenizer.concrete = true;
        State::Retry(StateName::TexMathFlowBeforeChunk)
    }
}

/// Strip at most the opening indentation from each content line.
pub fn content_start(tokenizer: &mut Tokenizer) -> State {
    if matches!(tokenizer.current, Some(b' ' | b'\t'))
        && tokenizer.tokenize_state.tex_math_flow_indent > 0
    {
        tokenizer.attempt(State::Next(StateName::TexMathFlowBeforeChunk), State::Nok);
        State::Retry(space_or_tab_min_max(
            tokenizer,
            0,
            tokenizer.tokenize_state.tex_math_flow_indent,
        ))
    } else {
        State::Retry(StateName::TexMathFlowBeforeChunk)
    }
}

/// Before a nonempty data chunk, close candidate, or container-aware break.
pub fn before_chunk(tokenizer: &mut Tokenizer) -> State {
    match tokenizer.current {
        None | Some(b'\n') => {
            tokenizer.check(
                State::Next(StateName::TexMathFlowLineEnding),
                State::Next(StateName::TexMathFlowAfter),
            );
            State::Retry(StateName::NonLazyContinuationStart)
        }
        Some(byte) if byte == tokenizer.tokenize_state.tex_math_flow_marker => {
            tokenizer.attempt(
                State::Next(StateName::TexMathFlowAfter),
                State::Next(StateName::TexMathFlowResume),
            );
            State::Retry(StateName::TexMathFlowCloseStart)
        }
        _ => {
            tokenizer.enter(Name::MathFlowChunk);
            State::Retry(StateName::TexMathFlowContent)
        }
    }
}

/// Raw TeX content. Never close a pre-attempt event inside an attempt.
pub fn content(tokenizer: &mut Tokenizer) -> State {
    match tokenizer.current {
        None | Some(b'\n') => {
            tokenizer.exit(Name::MathFlowChunk);
            State::Retry(StateName::TexMathFlowBeforeChunk)
        }
        Some(byte) if byte == tokenizer.tokenize_state.tex_math_flow_marker => {
            tokenizer.exit(Name::MathFlowChunk);
            State::Retry(StateName::TexMathFlowBeforeChunk)
        }
        Some(b'\\') => {
            tokenizer.consume();
            State::Next(StateName::TexMathFlowEscaped)
        }
        _ => {
            tokenizer.consume();
            State::Next(StateName::TexMathFlowContent)
        }
    }
}

/// A failed closer is data; pair a backslash with the following byte.
pub fn resume(tokenizer: &mut Tokenizer) -> State {
    tokenizer.enter(Name::MathFlowChunk);
    tokenizer.consume();
    State::Next(if tokenizer.tokenize_state.tex_math_flow_marker == b'\\' {
        StateName::TexMathFlowEscaped
    } else {
        StateName::TexMathFlowContent
    })
}

/// Paired characters (notably `\\[2mm]`) must stay intact.
pub fn escaped(tokenizer: &mut Tokenizer) -> State {
    if matches!(tokenizer.current, None | Some(b'\n')) {
        State::Retry(StateName::TexMathFlowContent)
    } else {
        tokenizer.consume();
        State::Next(StateName::TexMathFlowContent)
    }
}

/// Try a closing delimiter, including content before it on its line.
pub fn close_start(tokenizer: &mut Tokenizer) -> State {
    tokenizer.enter(Name::MathFlowFence);
    tokenizer.enter(Name::MathFlowFenceSequence);
    tokenizer.consume();
    State::Next(StateName::TexMathFlowCloseSecond)
}

/// The closing delimiter's second byte.
pub fn close_second(tokenizer: &mut Tokenizer) -> State {
    let expected = if tokenizer.tokenize_state.tex_math_flow_marker == b'\\' {
        b']'
    } else {
        b'$'
    };
    if tokenizer.current != Some(expected) {
        return State::Nok;
    }
    tokenizer.consume();
    tokenizer.exit(Name::MathFlowFenceSequence);
    State::Next(StateName::TexMathFlowCloseTrailing)
}

/// Closers must end their line (apart from horizontal whitespace).
pub fn close_trailing(tokenizer: &mut Tokenizer) -> State {
    match tokenizer.current {
        Some(b' ' | b'\t') => {
            tokenizer.consume();
            State::Next(StateName::TexMathFlowCloseTrailing)
        }
        None | Some(b'\n') => {
            tokenizer.exit(Name::MathFlowFence);
            State::Ok
        }
        _ => State::Nok,
    }
}

/// A non-lazy line inside the same container.
pub fn line_ending(tokenizer: &mut Tokenizer) -> State {
    tokenizer.enter(Name::LineEnding);
    tokenizer.consume();
    tokenizer.exit(Name::LineEnding);
    State::Next(StateName::TexMathFlowContentStart)
}

/// Finish a closed display or an unclosed fence at its container's end.
pub fn after(tokenizer: &mut Tokenizer) -> State {
    tokenizer.exit(Name::MathFlow);
    tokenizer.tokenize_state.tex_math_flow_marker = 0;
    tokenizer.tokenize_state.tex_math_flow_indent = 0;
    tokenizer.tokenize_state.tex_math_flow_fenced = false;
    tokenizer.interrupt = false;
    tokenizer.concrete = false;
    State::Ok
}
