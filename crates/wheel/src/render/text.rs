//! The text formats the wheel draws with, built once per size.

use windows::Win32::Graphics::DirectWrite::{
    DWRITE_FACTORY_TYPE_SHARED, DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_NORMAL,
    DWRITE_FONT_WEIGHT, DWRITE_FONT_WEIGHT_NORMAL, DWRITE_FONT_WEIGHT_SEMI_BOLD,
    DWRITE_PARAGRAPH_ALIGNMENT_CENTER, DWRITE_TEXT_ALIGNMENT_CENTER, DWRITE_WORD_WRAPPING_WRAP,
    DWriteCreateFactory, IDWriteFactory, IDWriteTextFormat,
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
}

impl Fonts {
    pub fn new(metrics: Metrics) -> Result<Self> {
        // SAFETY: the shared factory is reference counted by windows-rs.
        let factory: IDWriteFactory = unsafe { DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED) }?;

        Ok(Self {
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
        })
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
