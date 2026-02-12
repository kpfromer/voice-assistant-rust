/// Converts an integer to its human-readable word representation.
///
/// # Examples
/// ```
/// assert_eq!(int_to_words(1), "one");
/// assert_eq!(int_to_words(42), "forty two");
/// assert_eq!(int_to_words(100), "one hundred");
/// ```
pub fn int_to_words<T: Into<i32>>(n: T) -> String {
    let n = n.into();

    if n == 0 {
        return "zero".to_string();
    }

    if n < 0 {
        return format!("negative {}", int_to_words(-n));
    }

    if n < 20 {
        return match n {
            1 => "one",
            2 => "two",
            3 => "three",
            4 => "four",
            5 => "five",
            6 => "six",
            7 => "seven",
            8 => "eight",
            9 => "nine",
            10 => "ten",
            11 => "eleven",
            12 => "twelve",
            13 => "thirteen",
            14 => "fourteen",
            15 => "fifteen",
            16 => "sixteen",
            17 => "seventeen",
            18 => "eighteen",
            19 => "nineteen",
            _ => unreachable!(),
        }
        .to_string();
    }

    if n < 100 {
        let tens = n / 10;
        let ones = n % 10;

        let tens_word = match tens {
            2 => "twenty",
            3 => "thirty",
            4 => "forty",
            5 => "fifty",
            6 => "sixty",
            7 => "seventy",
            8 => "eighty",
            9 => "ninety",
            _ => unreachable!(),
        };

        if ones == 0 {
            tens_word.to_string()
        } else {
            format!("{} {}", tens_word, int_to_words(ones))
        }
    } else if n < 1000 {
        let hundreds = n / 100;
        let remainder = n % 100;

        let mut result = format!("{} hundred", int_to_words(hundreds));
        if remainder > 0 {
            result.push_str(" ");
            result.push_str(&int_to_words(remainder));
        }
        result
    } else if n < 1_000_000 {
        let thousands = n / 1000;
        let remainder = n % 1000;

        let mut result = format!("{} thousand", int_to_words(thousands));
        if remainder > 0 {
            result.push_str(" ");
            result.push_str(&int_to_words(remainder));
        }
        result
    } else {
        // For numbers beyond 999,999, return the number as a string
        // This is a reasonable limit for voice assistant use cases
        n.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_single_digits() {
        assert_eq!(int_to_words(0), "zero");
        assert_eq!(int_to_words(1), "one");
        assert_eq!(int_to_words(2), "two");
        assert_eq!(int_to_words(9), "nine");
    }

    #[test]
    fn test_teens() {
        assert_eq!(int_to_words(10), "ten");
        assert_eq!(int_to_words(11), "eleven");
        assert_eq!(int_to_words(15), "fifteen");
        assert_eq!(int_to_words(19), "nineteen");
    }

    #[test]
    fn test_tens() {
        assert_eq!(int_to_words(20), "twenty");
        assert_eq!(int_to_words(30), "thirty");
        assert_eq!(int_to_words(42), "forty two");
        assert_eq!(int_to_words(99), "ninety nine");
    }

    #[test]
    fn test_hundreds() {
        assert_eq!(int_to_words(100), "one hundred");
        assert_eq!(int_to_words(101), "one hundred one");
        assert_eq!(int_to_words(123), "one hundred twenty three");
        assert_eq!(int_to_words(999), "nine hundred ninety nine");
    }

    #[test]
    fn test_thousands() {
        assert_eq!(int_to_words(1000), "one thousand");
        assert_eq!(int_to_words(1001), "one thousand one");
        assert_eq!(int_to_words(1234), "one thousand two hundred thirty four");
        assert_eq!(int_to_words(9999), "nine thousand nine hundred ninety nine");
    }

    #[test]
    fn test_negative() {
        assert_eq!(int_to_words(-1), "negative one");
        assert_eq!(int_to_words(-42), "negative forty two");
    }
}
