/// Macro to generate a compile-time constant array containing all permutations
/// of multiple enums.
///
/// This macro:
/// - Accepts a list of tuples (`($name, $enum)`) where:
///   - `$name` is a string representing the key for the enum.
///   - `$enum` is an enum type that implements `strum::EnumVariantNames`.
/// - Computes **all possible permutations** of the provided enums at **compile-time**.
/// - Generates a uniquely named constant in the format `<ENUM1_ENUM2_PERMUTATIONS>`.
///
/// # Example
/// ```rust, ignore
/// #[derive(strum::EnumVariantNames)]
/// enum Color {
///     Red,
///     Green,
///     Blue,
/// }
///
/// #[derive(strum::EnumVariantNames)]
/// enum Size {
///     Small,
///     Medium,
///     Large,
/// }
///
/// generate_permutations!(
///     ("color", Color),
///     ("size", Size),
/// );
/// ```
///
/// # Output
/// ```text
/// [("color", "Red"), ("size", "Small")]
/// [("color", "Red"), ("size", "Medium")]
/// [("color", "Red"), ("size", "Large")]
/// [("color", "Green"), ("size", "Small")]
/// [("color", "Green"), ("size", "Medium")]
/// [("color", "Green"), ("size", "Large")]
/// [("color", "Blue"), ("size", "Small")]
/// [("color", "Blue"), ("size", "Medium")]
/// [("color", "Blue"), ("size", "Large")]
/// ```
#[macro_export]
macro_rules! generate_permutations {
    // Macro pattern to accept multiple (name, enum type) pairs.
    ($(($name:expr, $enum:ty)),* $(,)?) => {
        // Using the `paste` crate to concatenate enum names into a single identifier.
        $crate::paste::paste! {
            // The generated constant containing all permutations.
            // The constant name is dynamically generated by combining 
            // the uppercase names of the provided enums.
            pub const [<$($enum:upper _)* PERMUTATIONS>]: [[(&'static str, &'static str); {
                // Compute the number of enums being used in the permutations.
                [$($enum::VARIANTS.len()),*].len()
            }]; {
                // Compute the total number of permutations.
                let mut total_size = 1;
                $( total_size *= $enum::VARIANTS.len(); )*
                total_size
            }] = {
                /// An array holding references to the variant names of each enum.
                const ENUM_VARIANTS: [&'static [&'static str]; {
                    [$($enum::VARIANTS.len()),*].len()
                }] = [
                    $($enum::VARIANTS),*
                ];

                /// A constant representing the total number of permutations.
                const TOTAL_SIZE: usize = {
                    let mut product = 1;
                    $( product *= $enum::VARIANTS.len(); )*
                    product
                };

                /// A compile-time function to generate all permutations.
                /// 
                /// # Arguments
                /// * `variants` - A reference to an array of slices, where each slice contains the variants of an enum.
                /// * `names` - A reference to an array of enum names.
                ///
                /// # Returns
                /// A 2D array where each row represents a unique combination of variant names across the provided enums.
                const fn expand<const N: usize>(
                    variants: [&'static [&'static str]; N],
                    names: [&'static str; N]
                ) -> [[(&'static str, &'static str); N]; TOTAL_SIZE] {
                    // The output array containing all possible variant name combinations.
                    let mut results: [[(&'static str, &'static str); N]; TOTAL_SIZE] =
                        [[("", ""); N]; TOTAL_SIZE];

                    let mut index = 0;
                    let mut counters = [0; N];

                    // Iterate over all possible permutations.
                    while index < TOTAL_SIZE {
                        let mut row: [(&'static str, &'static str); N] = [("", ""); N];
                        let mut i = 0;

                        // Assign the correct variant name to each position in the row.
                        while i < N {
                            row[i] = (names[i], variants[i][counters[i]]);
                            i += 1;
                        }

                        results[index] = row;
                        index += 1;

                        // Carry propagation for multi-dimensional iteration.
                        let mut carry = true;
                        let mut j = 0;

                        while j < N && carry {
                            counters[j] += 1;
                            if counters[j] < variants[j].len() {
                                carry = false;
                            } else {
                                counters[j] = 0;
                            }
                            j += 1;
                        }
                    }

                    results
                }

                // Calls `expand` to generate the final constant containing all permutations.
                expand(ENUM_VARIANTS, [$($name),*])
            };
        }
    };
}

#[cfg(test)]
#[path = "label_utils_test.rs"]
mod label_utils_test;
