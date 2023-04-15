///! Finite field element management
use rug::{ops::Pow, Integer};
use std::ops::{Add, Div, Mul, Sub};

#[derive(Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FieldElement {
    num: Integer,
    prime: u32,
}

impl FieldElement {
    // Create a new FieldElement
    pub fn new(num: Integer, prime: u32) -> FieldElement {
        if prime < 2 {
            panic!("invalid base: it must be equal or greater than 2");
        }

        FieldElement {
            num,
            prime,
            ..Default::default()
        }
    }

    // Exp operator (Fermat's lIttle Theorem)
    pub fn pow(self, exponent: i32) -> FieldElement {
        let exp = exponent.rem_euclid(self.prime as i32 - 1) as u32;

        let (_q, rem) = (self.num.clone().pow(exp)).div_rem_euc(Into::into(self.prime));
        return FieldElement::new(rem, self.prime);
    }
}

impl Clone for FieldElement {
    fn clone(&self) -> FieldElement {
        return FieldElement::new(self.num.clone(), self.prime);
    }
}

impl Add for FieldElement {
    type Output = Self;

    // Add operator
    fn add(self, other: Self) -> Self {
        if self.prime != other.prime {
            panic!("cannot add two numbers in different fields");
        }

        let s = &self.num + &other.num;
        let (_q, rem) = Integer::from(s).div_rem_euc(Into::into(self.prime));

        return FieldElement::new(rem, self.prime);
    }
}

impl Sub for FieldElement {
    type Output = Self;

    // Sub operator
    fn sub(self, other: Self) -> Self {
        if self.prime != other.prime {
            panic!("cannot sub two numbers in different fields");
        }

        let s = &self.num - &other.num;
        let (_q, rem) = Integer::from(s).div_rem_euc(Into::into(self.prime));
        return FieldElement::new(rem, self.prime);
    }
}

impl<'a, 'b> Sub<&'b FieldElement> for &'a FieldElement {
    type Output = FieldElement;

    fn sub(self, other: &'b FieldElement) -> FieldElement {
        if self.prime != other.prime {
            panic!("cannot sub two numbers in different fields");
        }

        let s = &self.num - &other.num;
        let (_q, rem) = Integer::from(s).div_rem_euc(Into::into(self.prime));
        return FieldElement::new(rem, self.prime);
    }
}

impl<'a, 'b> Sub<&'b FieldElement> for FieldElement {
    type Output = FieldElement;

    fn sub(self, other: &'b FieldElement) -> FieldElement {
        if self.prime != other.prime {
            panic!("cannot sub two numbers in different fields");
        }

        let s = &self.num - &other.num;
        let (_q, rem) = Integer::from(s).div_rem_euc(Into::into(self.prime));
        return FieldElement::new(rem, self.prime);
    }
}

impl Mul for FieldElement {
    type Output = Self;

    // Mul operator
    fn mul(self, other: Self) -> Self {
        if self.prime != other.prime {
            panic!("cannot mul two numbers in different fields");
        }

        let s = &self.num * &other.num;
        let (_q, rem) = Integer::from(s).div_rem_euc(Into::into(self.prime));
        return FieldElement::new(rem, self.prime);
    }
}

impl<'a, 'b> Mul<&'b FieldElement> for &'a FieldElement {
    type Output = FieldElement;

    fn mul(self, other: &'b FieldElement) -> FieldElement {
        if self.prime != other.prime {
            panic!("cannot div two numbers in different fields");
        }

        let s = &self.num * &other.num;
        let (_q, rem) = Integer::from(s).div_rem_euc(Into::into(self.prime));
        return FieldElement::new(rem, self.prime);
    }
}

impl<'a, 'b> Mul<&'b FieldElement> for FieldElement {
    type Output = FieldElement;

    fn mul(self, other: &'b FieldElement) -> FieldElement {
        if self.prime != other.prime {
            panic!("cannot div two numbers in different fields");
        }

        let s = &self.num * &other.num;
        let (_q, rem) = Integer::from(s).div_rem_euc(Into::into(self.prime));
        return FieldElement::new(rem, self.prime);
    }
}

impl Div for FieldElement {
    type Output = Self;

    // Div operator
    fn div(self, other: Self) -> Self {
        if self.prime != other.prime {
            panic!("cannot div two numbers in different fields");
        }

        let s = &self.num * (other.num.pow(self.prime - 2));
        let (_q, rem) = s.div_rem_euc(Into::into(self.prime));

        return FieldElement::new(rem, self.prime);
    }
}

impl<'a, 'b> Div<&'b FieldElement> for &'a FieldElement {
    type Output = FieldElement;

    fn div(self, other: &'b FieldElement) -> FieldElement {
        if self.prime != other.prime {
            panic!("cannot div two numbers in different fields");
        }

        let s = &self.num * (other.num.clone().pow(self.prime - 2));
        let (_q, rem) = s.div_rem_euc(Into::into(self.prime));

        return FieldElement::new(rem, self.prime);
    }
}

impl<'a, 'b> Mul<&'b FieldElement> for i32 {
    type Output = FieldElement;

    fn mul(self, other: &'b FieldElement) -> FieldElement {
        let s = self * &other.num;
        let (_q, rem) = Integer::from(s).div_rem_euc(Into::into(other.prime));

        return FieldElement::new(rem, other.prime);
    }
}

impl Mul<FieldElement> for i32 {
    type Output = FieldElement;

    fn mul(self, other: FieldElement) -> FieldElement {
        let s = self * &other.num;
        let (_q, rem) = Integer::from(s).div_rem_euc(Into::into(other.prime));

        return FieldElement::new(rem, other.prime);
    }
}

impl Mul<i32> for FieldElement {
    type Output = FieldElement;

    fn mul(self, other: i32) -> FieldElement {
        let s = &self.num * other;
        let (_q, rem) = Integer::from(s).div_rem_euc(Into::into(self.prime));

        return FieldElement::new(rem, self.prime);
    }
}

#[cfg(test)]
mod field_element_test {
    use crate::field_element::*;

    #[test]
    fn fields_are_equals() {
        let field1 = FieldElement::new(Integer::from(1), 2);
        let field2 = FieldElement::new(Integer::from(1), 2);

        assert_eq!(field1, field2);
    }

    #[test]
    fn fields_are_different_by_num() {
        let field1 = FieldElement::new(Integer::from(1), 2);
        let field2 = FieldElement::new(Integer::from(2), 2);

        assert_ne!(field1, field2);
    }

    #[test]
    fn fields_are_different_by_prime() {
        let field1 = FieldElement::new(Integer::from(1), 2);
        let field2 = FieldElement::new(Integer::from(1), 3);

        assert_ne!(field1, field2);
    }

    #[test]
    fn adding_fields() {
        let field1 = FieldElement::new(Integer::from(7), 13);
        let field2 = FieldElement::new(Integer::from(12), 13);
        let field3 = FieldElement::new(Integer::from(6), 13);

        assert_eq!(field1 + field2, field3);
    }

    #[test]
    #[should_panic(expected = "cannot add two numbers in different fields")]
    fn adding_different_fields() {
        let field1 = FieldElement::new(Integer::from(7), 10);
        let field2 = FieldElement::new(Integer::from(12), 13);

        let _r_ = field1 + field2;
    }

    #[test]
    fn subtracting_fields() {
        let field1 = FieldElement::new(Integer::from(76), 13);
        let field2 = FieldElement::new(Integer::from(12), 13);
        let field3 = FieldElement::new(Integer::from(12), 13);

        assert_eq!(field1 - field2, field3);
    }

    #[test]
    #[should_panic(expected = "cannot sub two numbers in different fields")]
    fn subtracting_different_fields() {
        let field1 = FieldElement::new(Integer::from(76), 10);
        let field2 = FieldElement::new(Integer::from(12), 13);

        let _r_ = field1 - field2;
    }

    #[test]
    fn multiplying_fields() {
        let field1 = FieldElement::new(Integer::from(3), 13);
        let field2 = FieldElement::new(Integer::from(12), 13);
        let field3 = FieldElement::new(Integer::from(10), 13);

        assert_eq!(field1 * field2, field3);
    }

    #[test]
    #[should_panic(expected = "cannot mul two numbers in different fields")]
    fn multiplying_different_fields() {
        let field1 = FieldElement::new(Integer::from(76), 10);
        let field2 = FieldElement::new(Integer::from(12), 13);

        let _r_ = field1 * field2;
    }

    #[test]
    fn dividing_fields_1() {
        let field1 = FieldElement::new(Integer::from(3), 31);
        let field2 = FieldElement::new(Integer::from(24), 31);
        let field3 = FieldElement::new(Integer::from(4), 31);

        assert_eq!(field1 / field2, field3);
    }

    #[test]
    fn dividing_fields_2() {
        let field1 = FieldElement::new(Integer::from(3), 31);
        let field2 = FieldElement::new(Integer::from(24), 31);
        let field3 = FieldElement::new(Integer::from(4), 31);

        assert_eq!(field1 / field2, field3);
    }

    #[test]
    #[should_panic(expected = "cannot div two numbers in different fields")]
    fn dividing_different_fields() {
        let field1 = FieldElement::new(Integer::from(76), 10);
        let field2 = FieldElement::new(Integer::from(12), 13);

        let _r_ = field1 / field2;
    }

    #[test]
    fn exponentiationing_fields() {
        let field1 = FieldElement::new(Integer::from(3), 13);
        let field2 = FieldElement::new(Integer::from(1), 13);

        assert_eq!(field1.pow(3), field2);
    }

    #[test]
    fn exercise8_1() {
        let field1 = FieldElement::new(Integer::from(3), 31);
        let field2 = FieldElement::new(Integer::from(24), 31);
        let field3 = FieldElement::new(Integer::from(4), 31);

        assert_eq!(field1 * field2.pow(-1), field3);
    }

    #[test]
    fn exercise8_2() {
        let field1 = FieldElement::new(Integer::from(17), 31);
        let field2 = FieldElement::new(Integer::from(29), 31);

        assert_eq!(field1.pow(-3), field2);
    }

    #[test]
    fn exercise8_3() {
        let field1 = FieldElement::new(Integer::from(4), 31);
        let field2 = FieldElement::new(Integer::from(11), 31);
        let field3 = FieldElement::new(Integer::from(13), 31);

        assert_eq!(field1.pow(-4) * field2, field3);
    }

    #[test]
    fn exponentiationing_a_serie_7() {
        let v = a_serie(7);
        assert_eq!(v, a_vector_of_ones(7))
    }

    #[test]
    fn exponentiationing_a_serie_11() {
        let v = a_serie(11);
        assert_eq!(v, a_vector_of_ones(11))
    }

    #[test]
    fn exponentiationing_a_serie_17() {
        let v = a_serie(17);
        assert_eq!(v, a_vector_of_ones(17))
    }

    #[test]
    fn exponentiationing_a_serie_31() {
        let v = a_serie(31);
        assert_eq!(v, a_vector_of_ones(31))
    }

    fn a_vector_of_ones(p: u32) -> Vec<FieldElement> {
        let mut v = vec![];

        for _i in 1..p {
            v.push(FieldElement::new(Integer::from(1), p));
        }

        return v;
    }

    fn a_serie(p: u32) -> Vec<FieldElement> {
        let mut v = vec![];

        for i in 1..p {
            v.push(FieldElement::new(Integer::from(i), p).pow(p as i32 - 1));
        }

        return v;
    }
}
