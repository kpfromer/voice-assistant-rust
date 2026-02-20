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

/// Converts a word-form number string to an integer.
/// Handles: "five" → 5, "twenty three" → 23, "one hundred forty two" → 142
/// Also handles raw digit strings like "5" or "23".
pub fn words_to_int(s: &str) -> Option<u64> {
    let s = s.trim().to_lowercase();

    // Try parsing as a raw number first
    if let Ok(n) = s.parse::<u64>() {
        return Some(n);
    }

    let words: Vec<&str> = s.split_whitespace().collect();
    if words.is_empty() {
        return None;
    }

    let mut total: u64 = 0;
    let mut current: u64 = 0;

    for word in &words {
        match *word {
            "zero" => current += 0,
            "one" | "a" => current += 1,
            "two" => current += 2,
            "three" => current += 3,
            "four" => current += 4,
            "five" => current += 5,
            "six" => current += 6,
            "seven" => current += 7,
            "eight" => current += 8,
            "nine" => current += 9,
            "ten" => current += 10,
            "eleven" => current += 11,
            "twelve" => current += 12,
            "thirteen" => current += 13,
            "fourteen" => current += 14,
            "fifteen" => current += 15,
            "sixteen" => current += 16,
            "seventeen" => current += 17,
            "eighteen" => current += 18,
            "nineteen" => current += 19,
            "twenty" => current += 20,
            "thirty" => current += 30,
            "forty" => current += 40,
            "fifty" => current += 50,
            "sixty" => current += 60,
            "seventy" => current += 70,
            "eighty" => current += 80,
            "ninety" => current += 90,
            "hundred" => current *= 100,
            "thousand" => {
                current *= 1000;
                total += current;
                current = 0;
            }
            "and" => {} // skip "and" in "one hundred and five"
            _ => {
                // Try parsing individual word as digit
                if let Ok(n) = word.parse::<u64>() {
                    current += n;
                } else {
                    return None;
                }
            }
        }
    }

    total += current;

    if total == 0 && !words.contains(&"zero") {
        return None;
    }

    Some(total)
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

    #[test]
    fn test_words_to_int_single_digits() {
        assert_eq!(words_to_int("zero"), Some(0));
        assert_eq!(words_to_int("one"), Some(1));
        assert_eq!(words_to_int("five"), Some(5));
        assert_eq!(words_to_int("nine"), Some(9));
    }

    #[test]
    fn test_words_to_int_teens() {
        assert_eq!(words_to_int("ten"), Some(10));
        assert_eq!(words_to_int("eleven"), Some(11));
        assert_eq!(words_to_int("fifteen"), Some(15));
        assert_eq!(words_to_int("nineteen"), Some(19));
    }

    #[test]
    fn test_words_to_int_tens() {
        assert_eq!(words_to_int("twenty"), Some(20));
        assert_eq!(words_to_int("forty two"), Some(42));
        assert_eq!(words_to_int("ninety nine"), Some(99));
    }

    #[test]
    fn test_words_to_int_hundreds() {
        assert_eq!(words_to_int("one hundred"), Some(100));
        assert_eq!(words_to_int("one hundred twenty three"), Some(123));
        assert_eq!(words_to_int("two hundred and five"), Some(205));
    }

    #[test]
    fn test_words_to_int_thousands() {
        assert_eq!(words_to_int("one thousand"), Some(1000));
        assert_eq!(words_to_int("one thousand two hundred thirty four"), Some(1234));
    }

    #[test]
    fn test_words_to_int_raw_digits() {
        assert_eq!(words_to_int("5"), Some(5));
        assert_eq!(words_to_int("42"), Some(42));
        assert_eq!(words_to_int("100"), Some(100));
    }

    #[test]
    fn test_words_to_int_a_as_one() {
        assert_eq!(words_to_int("a"), Some(1));
    }

    #[test]
    fn test_words_to_int_invalid() {
        assert_eq!(words_to_int("hello"), None);
        assert_eq!(words_to_int(""), None);
    }
}
