# Serial numbers

Tickets, raffle cards, labels and vouchers often need a number on each
copy, or a code nobody can guess. TraceDraw makes a whole set at once, each
number on its own background shape if you like, laid out in a grid ready
to print or cut.

## Inserting numbers {#insert}

Choose [[menu.object > menu.object.insert > menu.object.insert_serials]] to
open [[serials.title]], then pick what to make:

- [[serials.sequence]] counts from [[serials.start]] to [[serials.end]] by
  [[serials.step]]. [[serials.digits]] pads the shorter numbers with zeros,
  so 2 digits from 1 to 99 gives 01, 02 … 99. An end below the start
  counts down.
- [[serials.random]] makes [[serials.count]] codes of [[serials.length]]
  characters from [[serials.charset]]: digits, letters and digits (without
  0, O, 1 and I, which are easy to mistake for each other) or hexadecimal.
  No code repeats, and every insert makes a new set.

[[serials.prefix]] and [[serials.suffix]] add the same text to every number,
such as `No. ` or `-A`.

[[serials.backdrop]] puts a shape behind each number: a circle, a star, a
burst, a diamond, or a guilloche, a rosette of fine looping lines that is
hard to copy, for tickets and vouchers. [[serials.size]] is the width and
height of each shape; the numbers are sized so the longest fits inside it.
[[serials.columns]] and [[serials.gap]] set the grid, and
[[serials.text_color]] and [[serials.back_color]] the colors. The line under
the settings shows how many numbers you will get and how the first ones
read.

Click OK. The numbers come in as one group in the middle of the page,
selected, with one small group (the shape and its number) for each. Every
number is ordinary text, so you can ungroup the set, change the font or
edit a number. The dialog keeps its settings for the next time.

The shape generator's guilloche patterns and frames make larger
security backgrounds to put behind a set of numbers.
