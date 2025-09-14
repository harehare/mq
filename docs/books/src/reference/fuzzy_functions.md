# Fuzzy Functions

The Fuzzy module provides functions for fuzzy string matching and similarity calculations.

## Including the Fuzzy Module

To use the Fuzzy functions, include the module at the top of your mq script:

```mq
include "fuzzy"
```

## Functions

### `levenshtein(s1, s2)`

Calculates the Levenshtein distance between two strings. The Levenshtein distance is the minimum number of single-character edits (insertions, deletions, or substitutions) required to change one string into the other.

**Parameters:**
- `s1`: First string to compare
- `s2`: Second string to compare

**Returns:**
- Integer representing the Levenshtein distance (0 means strings are identical)

**Example:**
```mq
include "fuzzy"

# Calculate Levenshtein distance
| levenshtein("hello", "hallo")
# Returns: 1

| levenshtein("kitten", "sitting")
# Returns: 3

| levenshtein("identical", "identical")
# Returns: 0

| levenshtein("", "abc")
# Returns: 3
```

### `jaro(s1, s2)`

Calculates the Jaro distance between two strings. The Jaro distance is a measure of similarity between strings, ranging from 0.0 (no similarity) to 1.0 (exact match).

**Parameters:**
- `s1`: First string to compare
- `s2`: Second string to compare

**Returns:**
- Float between 0.0 and 1.0 (1.0 indicates exact match)

**Example:**
```mq
include "fuzzy"

# Calculate Jaro distance
| jaro("hello", "hallo")
# Returns: 0.866667

| jaro("martha", "marhta")
# Returns: 0.9444444444444444

| jaro("identical", "identical")
# Returns: 1.0

| jaro("", "abc")
# Returns: 0.0
```

### `jaro_winkler(s1, s2)`

Calculates the Jaro-Winkler distance between two strings. This is a variant of the Jaro distance with a prefix scale that gives more favorable ratings to strings with common prefixes.

**Parameters:**
- `s1`: First string to compare
- `s2`: Second string to compare

**Returns:**
- Float between 0.0 and 1.0 (1.0 indicates exact match)

**Example:**
```mq
include "fuzzy"

# Calculate Jaro-Winkler distance
| jaro_winkler("hello", "hallo")
# Returns: 0.8666666666666667

| jaro_winkler("martha", "marhta")
# Returns: 0.9611111111111111

| jaro_winkler("prefix_test", "prefix_example")
# Returns: 0.8571428571428571

| jaro_winkler("identical", "identical")
# Returns: 1.0
```

### `fuzzy_match(query, candidates)`

Performs fuzzy matching on an array of strings using the Jaro-Winkler distance algorithm. Returns results sorted by similarity score in descending order.

**Parameters:**
- `query`: String to search for
- `candidates`: Array of strings to search within, or a single string

**Returns:**
- Array of objects with `text` and `score` properties, sorted by best match first

**Example:**
```mq
include "fuzzy"

# Fuzzy match with multiple candidates
| fuzzy_match("hello", ["hallo", "hello", "hi", "help"])
# Returns: [
#   {"text": "hello", "score": 1.0},
#   {"text": "hallo", "score": 0.8666666666666667},
#   {"text": "help", "score": 0.7333333333333334},
#   {"text": "hi", "score": 0.0}
# ]

# Fuzzy match with single candidate
| fuzzy_match("test", "testing")
# Returns: [{"text": "testing", "score": 0.8095238095238095}]
```

### `fuzzy_match_levenshtein(query, candidates)`

Performs fuzzy matching using Levenshtein distance. Returns results sorted by distance (lower distance means better match).

**Parameters:**
- `query`: String to search for
- `candidates`: Array of strings to search within

**Returns:**
- Array of objects with `text` and `score` properties, sorted by lowest distance first

**Example:**
```mq
include "fuzzy"

# Fuzzy match using Levenshtein distance
| fuzzy_match_levenshtein("hello", ["hallo", "hello", "hi", "help"])
# Returns: [
#   {"text": "hello", "score": 0},
#   {"text": "hallo", "score": 1},
#   {"text": "help", "score": 2},
#   {"text": "hi", "score": 4}
# ]
```

### `fuzzy_match_jaro(query, candidates)`

Performs fuzzy matching using the Jaro distance algorithm. Returns results sorted by similarity score in descending order.

**Parameters:**
- `query`: String to search for
- `candidates`: Array of strings to search within

**Returns:**
- Array of objects with `text` and `score` properties, sorted by best match first

**Example:**
```mq
include "fuzzy"

# Fuzzy match using Jaro distance
| fuzzy_match_jaro("hello", ["hallo", "hello", "hi", "help"])
# Returns: [
#   {"text": "hello", "score": 1.0},
#   {"text": "hallo", "score": 0.8666666666666667},
#   {"text": "help", "score": 0.7333333333333334},
#   {"text": "hi", "score": 0.0}
# ]
```

### `fuzzy_filter(query, candidates, threshold)`

Filters candidates by minimum fuzzy match score using Jaro-Winkler distance. Only returns matches that meet or exceed the specified threshold.

**Parameters:**
- `query`: String to search for
- `candidates`: Array of strings to search within
- `threshold`: Minimum score threshold (0.0 to 1.0)

**Returns:**
- Array of objects with `text` and `score` properties for matches above threshold

**Example:**
```mq
include "fuzzy"

# Filter matches with minimum threshold
| fuzzy_filter("hello", ["hallo", "hello", "hi", "help"], 0.7)
# Returns: [
#   {"text": "hello", "score": 1.0},
#   {"text": "hallo", "score": 0.8666666666666667},
#   {"text": "help", "score": 0.7333333333333334}
# ]

# Filter with high threshold
| fuzzy_filter("hello", ["hallo", "hello", "hi", "help"], 0.9)
# Returns: [
#   {"text": "hello", "score": 1.0}
# ]
```

### `fuzzy_best_match(query, candidates)`

Finds the best fuzzy match from candidates using Jaro-Winkler distance.

**Parameters:**
- `query`: String to search for
- `candidates`: Array of strings to search within

**Returns:**
- Object with `text` and `score` properties for the best match, or `None` if no matches found

**Example:**
```mq
include "fuzzy"

# Find best match
| fuzzy_best_match("hello", ["hallo", "hi", "help"])
# Returns: {"text": "hallo", "score": 0.8666666666666667}

# No matches case
| fuzzy_best_match("xyz", [])
# Returns: None
```
