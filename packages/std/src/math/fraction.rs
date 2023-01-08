use crate::Uint256;

/// A fraction `p`/`q` with integers `p` and `q`.
///
/// `p` is called the numerator and `q` is called the denominator.
pub trait Fraction<T>: Sized {
    /// Returns the numerator `p`
    fn numerator(&self) -> T;
    /// Returns the denominator `q`
    fn denominator(&self) -> T;

    /// Returns the multiplicative inverse `q/p` for fraction `p/q`.
    ///
    /// If `p` is zero, None is returned.
    fn inv(&self) -> Option<Self>;
}

impl<T: Copy + Into<Uint256>> Fraction<T> for (T, T) {
    fn numerator(&self) -> T {
        self.0
    }

    fn denominator(&self) -> T {
        self.1
    }

    fn inv(&self) -> Option<Self> {
        if self.numerator().into() == Uint256::zero() {
            None
        } else {
            Some((self.1, self.0))
        }
    }
}

#[macro_export]
macro_rules! impl_mul_fraction {
    ($Uint:ident) => {
        impl $Uint {
            pub fn checked_mul_floored<F: Fraction<T>, T: Into<$Uint>>(
                self,
                rhs: F,
            ) -> Result<Self, CheckedMultiplyFractionError> {
                let divisor = rhs.denominator().into();
                let res = self
                    .full_mul(rhs.numerator().into())
                    .checked_div(divisor.into())?;
                Ok(res.try_into()?)
            }

            pub fn mul_floored<F: Fraction<T>, T: Into<$Uint>>(self, rhs: F) -> Self {
                self.checked_mul_floored(rhs).unwrap()
            }

            pub fn checked_mul_ceil<F: Fraction<T>, T: Into<$Uint>>(
                self,
                rhs: F,
            ) -> Result<Self, CheckedMultiplyFractionError> {
                let divisor = rhs.denominator().into();
                let remainder = self
                    .full_mul(rhs.numerator().into())
                    .checked_rem(divisor.into())?;
                let floor_result = self.checked_mul_floored(rhs)?;
                if !remainder.is_zero() {
                    Ok($Uint::one().checked_add(floor_result)?)
                } else {
                    Ok(floor_result)
                }
            }

            pub fn mul_ceil<F: Fraction<T>, T: Into<$Uint>>(self, rhs: F) -> Self {
                self.checked_mul_ceil(rhs).unwrap()
            }
        }
    };
}

#[cfg(test)]
mod tests {
    use crate::{Fraction, Uint128, Uint64};

    #[test]
    fn fraction_tuple_methods() {
        let fraction = (Uint64::one(), Uint64::new(2));
        assert_eq!(Uint64::one(), fraction.numerator());
        assert_eq!(Uint64::new(2), fraction.denominator());
        assert_eq!(Some((Uint64::new(2), Uint64::one())), fraction.inv());
    }

    #[test]
    fn inverse_with_zero_denominator() {
        let fraction = (Uint128::zero(), Uint128::one());
        assert_eq!(None, fraction.inv());
    }
}
