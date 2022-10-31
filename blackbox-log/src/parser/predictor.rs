use core::ops::{Add, Div, Sub};

use super::{as_signed, as_unsigned, Headers, ParseResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Predictor {
    Zero = 0,
    Previous,
    StraightLine,
    Average2,
    MinThrottle,
    Motor0,
    Increment,
    HomeLat, // TODO: check that lat = 0, lon = 1
    FifteenHundred,
    VBatReference,
    LastMainFrameTime,
    MinMotor,
    // HomeLon = 256,
}

impl Predictor {
    #[allow(clippy::too_many_arguments)]
    pub fn apply(
        self,
        headers: &Headers,
        value: u32,
        signed: bool,
        current: &[u32],
        last: Option<u32>,
        last_last: Option<u32>,
        skipped_frames: u32,
    ) -> ParseResult<u32> {
        let _span = if signed {
            tracing::trace_span!(
                "Predictor::apply",
                ?self,
                value = as_signed(value),
                last = last.map(as_signed),
                last_last = last_last.map(as_signed),
                skipped_frames,
            )
        } else {
            tracing::trace_span!(
                "Predictor::apply",
                ?self,
                value,
                last,
                last_last,
                skipped_frames
            )
        };
        let _span = _span.enter();

        let diff = match self {
            Self::Zero => 0,
            Self::Previous => last.unwrap_or(0),
            Self::StraightLine => {
                if signed {
                    as_unsigned(straight_line(last.map(as_signed), last_last.map(as_signed)))
                } else {
                    straight_line(last, last_last)
                }
            }
            Self::Average2 => {
                if signed {
                    as_unsigned(average(last.map(as_signed), last_last.map(as_signed)))
                } else {
                    average(last, last_last)
                }
            }
            Self::MinThrottle => headers.min_throttle.into(),
            Self::Motor0 => headers.main_frames.get_motor_0_from(current)?,
            Self::Increment => {
                if signed {
                    1 + skipped_frames + last.unwrap_or(0)
                } else {
                    let skipped_frames = i32::try_from(skipped_frames)
                        .expect("never skip more than i32::MAX frames");
                    as_unsigned(1 + skipped_frames + as_signed(last.unwrap_or(0)))
                }
            }
            // Self::HomeLat => todo!(), // TODO: check that lat = 0, lon = 1
            Self::FifteenHundred => 1500,
            Self::VBatReference => headers.vbat_reference.into(),
            // Self::LastMainFrameTime => todo!(),
            Self::MinMotor => headers.motor_output_range.min().into(),
            // Self::HomeLon => todo!(),
            Self::HomeLat | Self::LastMainFrameTime => {
                tracing::warn!("found unimplemented predictor: {self:?}");
                0
            }
        };

        Ok(if signed {
            let signed = as_signed(value) + as_signed(diff);
            tracing::trace!(return = signed);
            as_unsigned(signed)
        } else {
            let x = value + diff;
            tracing::trace!(return = x);
            x
        })
    }

    pub(crate) fn from_num_str(s: &str) -> Option<Self> {
        match s {
            "0" => Some(Self::Zero),
            "1" => Some(Self::Previous),
            "2" => Some(Self::StraightLine),
            "3" => Some(Self::Average2),
            "4" => Some(Self::MinThrottle),
            "5" => Some(Self::Motor0),
            "6" => Some(Self::Increment),
            "7" => Some(Self::HomeLat), // TODO: check that lat = 0, lon = 1
            "8" => Some(Self::FifteenHundred),
            "9" => Some(Self::VBatReference),
            "10" => Some(Self::LastMainFrameTime),
            "11" => Some(Self::MinMotor),
            _ => None,
        }
    }
}

#[inline]
pub(crate) fn straight_line<T>(last: Option<T>, last_last: Option<T>) -> T
where
    T: TemporaryOverflow + Default,
    T::Wide: Copy + Sub<Output = T::Wide> + Add<Output = T::Wide> + PartialOrd,
{
    match (last, last_last) {
        (Some(last), Some(last_last)) => {
            let fallback = last;

            let result = {
                let last = last.widen();
                let last_last = last_last.widen();
                let sum = last + last;

                // Work around not being able to use .checked_sub()
                if !T::SIGNED && last_last > sum {
                    return fallback;
                }

                sum - last_last
            };
            T::try_from(result).unwrap_or(fallback)
        }
        (Some(last), None) => last,
        _ => T::default(),
    }
}

#[inline]
fn average<T>(last: Option<T>, last_last: Option<T>) -> T
where
    T: TemporaryOverflow + Copy + Default,
    T::Wide: Add<Output = T::Wide> + Div<Output = T::Wide> + From<u8>,
{
    let last = last.unwrap_or_default();
    last_last.map_or(last, |last_last| {
        T::truncate_from((last.widen() + last_last.widen()) / 2.into())
    })
}

pub(crate) trait TemporaryOverflow
where
    Self: Copy + TryFrom<Self::Wide>,
    Self::Wide: From<Self>,
{
    const SIGNED: bool;
    type Wide;
    fn truncate_from(larger: Self::Wide) -> Self;

    #[inline]
    fn widen(self) -> Self::Wide {
        self.into()
    }
}

macro_rules! impl_next_larger {
    ($($sign:ident $base:ident -> $wide:ident),+ $(,)?) => {
        $(impl_next_larger!($sign, $base, $wide);)+
    };
    (signed, $base:ident, $wide:ident) => { impl_next_larger!(true, $base, $wide); };
    (unsigned, $base:ident, $wide:ident) => { impl_next_larger!(false, $base, $wide); };
    ($sign:expr, $base:ident, $wide:ident) => {
        impl TemporaryOverflow for $base {
            const SIGNED: bool = $sign;
            type Wide = $wide;

            #[inline]
            fn truncate_from(wide: Self::Wide) -> Self {
                wide as $base
            }
        }
    }
}

impl_next_larger!(unsigned u8 -> u16, signed i8 -> i16);
impl_next_larger!(unsigned u16 -> u32, signed i16 -> i32);
impl_next_larger!(unsigned u32 -> u64, signed i32 -> i64);
impl_next_larger!(unsigned u64 -> u128, signed i64 -> i128);

#[cfg(test)]
mod tests {
    use test_case::case;

    #[case(None, None => 0)]
    #[case(Some(10), None => 10)]
    #[case(Some(-2), None => -2)]
    #[case(Some(12), Some(10) => 14)]
    #[case(Some(10), Some(12) => 8)]
    #[case(Some(0), Some(i8::MAX) => -i8::MAX)]
    #[case(Some(0), Some(i8::MIN) => 0 ; "underflow")]
    #[case(Some(126), Some(0) => 126 ; "overflow")]
    fn straight_line_signed(last: Option<i8>, last_last: Option<i8>) -> i8 {
        super::straight_line(last, last_last)
    }

    #[case(Some(2),Some(2) => 2)]
    #[case(Some(12), Some(10) => 14)]
    #[case(Some(10), Some(12) => 8)]
    #[case(Some(0), Some(u8::MIN) => 0 ; "underflow")]
    #[case(Some(u8::MAX - 1), Some(0) => 254 ; "overflow")]
    #[case(Some(0), Some(u8::MAX) => 0 ; "negative result")]
    fn straight_line_unsigned(last: Option<u8>, last_last: Option<u8>) -> u8 {
        super::straight_line(last, last_last)
    }

    #[case(None, None => 0)]
    #[case(Some(-1), None => -1)]
    #[case(Some(2), Some(-1) => 0)]
    #[case(Some(i32::MAX), Some(1) => 0x4000_0000 ; "overflow")]
    fn average_signed(last: Option<i32>, last_last: Option<i32>) -> i32 {
        super::average(last, last_last)
    }

    #[case(None, None => 0)]
    #[case(Some(1), None => 1)]
    #[case(Some(2), Some(10) => 6)]
    #[case(Some(u32::MAX), Some(1) => 0x8000_0000 ; "overflow")]
    fn average_unsigned(last: Option<u32>, last_last: Option<u32>) -> u32 {
        super::average(last, last_last)
    }
}