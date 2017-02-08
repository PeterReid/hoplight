An atom is a sequence of bytes.
A noun is an atom or a pair of nouns.

To encode an atom:
- If the atom is a single byte less than 190, the encoding is the byte itself
- If the atom is up to 64 bytes long, the encoding is the byte 190 + the length, followed by the contents.
- If the atom is longer than 64 bytes, then the length is broken into 7-bit chunks, with the least significant 7 bits first. The final byte has a 0 most significant bit and the others have 1 as their most significant bit.

A noun can be traversed.
- An atom is traversed by visiting the atom.
- A pair is traversed by visiting the pair itself, then its left child, and then its right child.

A noun is encoded as:
- An atom encoding the number of bytes that will be used to encode all atoms in the noun;
- The concatenation of the encodings of all atoms, ordered by traversing the noun.
- A concatenation of bits produced by traversing the noun and producing a `0` for each atom visited and a `1` for each pair visited.
  Within a byte, earlier-produced bits are encoded in less-significant bit positions.

```
    A noun is executed by running the `nock` function:
    1  ::    [a b c]           [a [b c]]
    2  ::    nock(a)           *a
    3  ::  
    4  ::    ?[a b]            0
    5  ::    ?a                1
    6  ::    +a                1 + a
    7  ::    =[a a]            0
    8  ::    =[a b]            1
    9  ::
    10 ::    /[1 a]            a
    11 ::    /[2 a b]          a
    12 ::    /[3 a b]          b
    13 ::    /[(a + a) b]      /[2 /[a b]]
    14 ::    /[(a + a + 1) b]  /[3 /[a b]]
    15 ::
    16 ::    _[a b]            _a concatenated with _b
    17 ::    _a                a
    18 ::    ^[a b]            [the initial `b` bytes of `a`, the remaining bytes of `a`]
    19 ::    ^[a [b c]]        [[x y] tail_2] where
    20 ::                         [x tail_1] = ^[a, b]
    21 ::                         [y tail_2] = ^[tail_1 c]
    22 ::    @[a b]            x where
    23 ::                         [x tail] = ^[_a, b]
    24 ::
    25 ::    *[a [b c] d]      [*[a b c] *[a d]]
    26 ::
    27 ::    *[a 0 b]          /[b a]
    28 ::    *[a 1 b]          b
    29 ::    *[a 2 b c]        *[*[a b] *[a c]]
    30 ::    *[a 3 b]          ?*[a b]
    31 ::    *[a 4 b]          +*[a b]
    32 ::    *[a 5 b]          =*[a b]
    33 ::
    34 ::    *[a 6 b c d]      *[a 2 [0 1] 2 [1 c d] [1 0] 2 [1 2 3] [1 0] 4 4 b]
    35 ::    *[a 7 b c]        *[a 2 b 1 c]
    36 ::    *[a 8 b c]        *[a 7 [[7 [0 1] b] 0 1] c]
    37 ::    *[a 9 b c]        *[a 7 c [2 [0 1] [0 b]]]
    38 ::    *[a 10 b]         hash(*[a b])
    39 ::    *[a 11 b]         0   and stores *[a b] under the hash of *[a b]
    40 ::    *[a 12 b]         1           if the hash *[a b] had not been stored
    41 ::                      [0 *[a X]]  if the hash *[a b] had X stored under it
    42 ::    *[a 13 b c]       0   and stores [*a c] under the key *[a b]
    43 ::    *[a 14 b]         1           if nothing had been stored under the key *[a b]
    44 ::                      [0 *[a X]]  if X had been stored under the key *[a b]
    45 ::    *[a 15 b]         A random atom of length *[a b]
    46 ::    *[a 16 b c]       @[*[a b] *[a c]]
```
