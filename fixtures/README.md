# Synthetic fixtures

`synthetic-v6.lua` is a hand-built test file. It follows the published AHDB schema. It contains no player data and is not a captured game file.

The expected scan has three listings and two variant keys. The first listing has 20 units and a total buyout of 20,000 copper. Its unit price is 1,000 copper. The second listing has no buyout. The third listing has a total buyout of 10,000,000,000 copper and three units. Its unit price must remain a rational number.

The history contains two recent points and one inherited UTC daily point. The source does not identify a realm for history. The parser leaves that realm empty.

Client compatibility, save and reload behavior, scan completeness, and source version are unverified. Replace no P0 release gate with this fixture.
