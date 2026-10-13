//! The text formats the wheel draws with, built once per size.

use windows::Win32::Graphics::DirectWrite::{
    DWRITE_FACTORY_TYPE_SHARED, DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_NORMAL,
    DWRITE_FONT_WEIGHT, DWRITE_FONT_WEIGHT_NORMAL, DWRITE_FONT_WEIGHT_SEMI_BOLD,
    DWRITE_PARAGRAPH_ALIGNMENT_CENTER, DWRITE_TEXT_ALIGNMENT_CENTER, DWRITE_TEXT_ALIGNMENT_LEADING,
    DWRITE_TRIMMING, DWRITE_TRIMMING_GRANULARITY_CHARACTER, DWRITE_WORD_WRAPPING_NO_WRAP,
    DWRITE_WORD_WRAPPING_WRAP, DWriteCreateFactory, IDWriteFactory, IDWriteTextFormat,
};
use windows::core::{HSTRING, Result};

use crate::theme::{self, Metrics};

/// Every format the wheel needs, all centred both ways.
#[derive(Debug)]
pub struct Fonts {
    label: IDWriteTextFormat,
    number: IDWriteTextFormat,
    hub_tab: IDWriteTextFormat,
    hub_clock: IDWriteTextFormat,
    hub_date: IDWriteTextFormat,
    cog: IDWriteTextFormat,
    glyph: IDWriteTextFormat,
    /// A form value at rest: one line, cut short with an ellipsis.
    value: IDWriteTextFormat,
    /// A form value being edited: one line, never cut, so it can scroll under the caret.
    editing: IDWriteTextFormat,
}

impl Fonts {
    /// Hands back the factory alongside the formats, because measuring text for the caret
    /// needs a layout and a layout needs the factory.
    pub fn new(metrics: Metrics) -> Result<(Self, IDWriteFactory)> {
        // SAFETY: the shared factory is reference counted by windows-rs.
        let factory: IDWriteFactory = unsafe { DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED) }?;

        let fonts = Self {
            label: centred(
                &factory,
                theme::LABEL_FONT,
                DWRITE_FONT_WEIGHT_SEMI_BOLD,
                metrics.label_size_px,
            )?,
            number: centred(
                &factory,
                theme::LABEL_FONT,
                DWRITE_FONT_WEIGHT_NORMAL,
                metrics.number_size_px,
            )?,
            hub_tab: centred(
                &factory,
                theme::LABEL_FONT,
                DWRITE_FONT_WEIGHT_SEMI_BOLD,
                metrics.hub_tab_size_px,
            )?,
            hub_clock: centred(
                &factory,
                theme::LABEL_FONT,
                DWRITE_FONT_WEIGHT_SEMI_BOLD,
                metrics.hub_clock_size_px,
            )?,
            hub_date: centred(
                &factory,
                theme::LABEL_FONT,
                DWRITE_FONT_WEIGHT_NORMAL,
                metrics.hub_date_size_px,
            )?,
            cog: centred(
                &factory,
                theme::ICON_FONT,
                DWRITE_FONT_WEIGHT_NORMAL,
                metrics.cog_size_px,
            )?,
            glyph: centred(
                &factory,
                theme::ICON_FONT,
                DWRITE_FONT_WEIGHT_NORMAL,
                metrics.number_size_px,
            )?,
            value: single_line(&factory, metrics.number_size_px, Ending::Ellipsis)?,
            editing: single_line(&factory, metrics.number_size_px, Ending::Full)?,
        };
        Ok((fonts, factory))
    }

    pub fn value(&self) -> &IDWriteTextFormat {
        &self.value
    }

    pub fn editing(&self) -> &IDWriteTextFormat {
        &self.editing
    }

    pub fn label(&self) -> &IDWriteTextFormat {
        &self.label
    }

    pub fn number(&self) -> &IDWriteTextFormat {
        &self.number
    }

    pub fn hub_tab(&self) -> &IDWriteTextFormat {
        &self.hub_tab
    }

    pub fn hub_clock(&self) -> &IDWriteTextFormat {
        &self.hub_clock
    }

    pub fn hub_date(&self) -> &IDWriteTextFormat {
        &self.hub_date
    }

    pub fn cog(&self) -> &IDWriteTextFormat {
        &self.cog
    }

    pub fn glyph(&self) -> &IDWriteTextFormat {
        &self.glyph
    }
}

/// How a single-line value ends when it is longer than its box.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Ending {
    Ellipsis,
    Full,
}

/// A left-aligned, one-line format for form values.
///
/// A long path wrapping onto a second line spills out of its row, so values stay on one
/// line and run east to west instead. Left alignment is also what the caret maths
/// assumes: it measures the text before the caret from the left edge of the box.
fn single_line(
    factory: &IDWriteFactory,
    size_px: f32,
    ending: Ending,
) -> Result<IDWriteTextFormat> {
    let format = centred(
        factory,
        theme::LABEL_FONT,
        DWRITE_FONT_WEIGHT_NORMAL,
        size_px,
    )?;

    // SAFETY: the format was just created and is not shared yet.
    unsafe {
        format.SetTextAlignment(DWRITE_TEXT_ALIGNMENT_LEADING)?;
        format.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP)?;
    }
    if ending == Ending::Full {
        return Ok(format);
    }

    // SAFETY: the sign is built from the format it is attached to, and both live on.
    unsafe {
        let sign = factory.CreateEllipsisTrimmingSign(&format)?;
        let trimming = DWRITE_TRIMMING {
            granularity: DWRITE_TRIMMING_GRANULARITY_CHARACTER,
            delimiter: 0,
            delimiterCount: 0,
        };
        format.SetTrimming(&raw const trimming, &sign)?;
    }
    Ok(format)
}

fn centred(
    factory: &IDWriteFactory,
    family: &str,
    weight: DWRITE_FONT_WEIGHT,
    size_px: f32,
) -> Result<IDWriteTextFormat> {
    let family = HSTRING::from(family);
    let locale = HSTRING::from(theme::LOCALE);

    // SAFETY: both strings stay live for the duration of the call.
    let format = unsafe {
        factory.CreateTextFormat(
            &family,
            None,
            weight,
            DWRITE_FONT_STYLE_NORMAL,
            DWRITE_FONT_STRETCH_NORMAL,
            size_px,
            &locale,
        )
    }?;

    // SAFETY: the format was just created and is not shared yet.
    unsafe {
        format.SetTextAlignment(DWRITE_TEXT_ALIGNMENT_CENTER)?;
        format.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER)?;
        // a long app name wraps inside its wedge rather than running over the dividers
        format.SetWordWrapping(DWRITE_WORD_WRAPPING_WRAP)?;
    }
    Ok(format)
}
