**Capstone, step 2 of 10.** The server holds keys, and each key holds one kind of value. This step runs one command against those keys. The starter holds the reference code of step 1.

**Spoiler warning.** The starter gives away step 1 of this capstone. It also gives away work from these problems. Solve them first if you have not.

- *A value inside a value, arriving in pieces*: step 1 shows how to parse nested input in pieces
- *Requests that arrive in pieces*: step 1 shows the same need-more outcomes

Write `Engine`. `execute(args, now)` runs one command and returns its reply.

`args[0]` is the command's name, in any letter case. Ignore `now` until the next step. Keys, values, fields and members are any bytes.

**Refusals.** A refused command changes nothing.

- An unknown name, or no arguments at all: an error starting `ERR unknown command`.
- Too few or too many arguments: `ERR wrong number of arguments for 'name' command`, with the name in lower case.
- A key that holds another kind of value: `WRONGTYPE Operation against a key holding the wrong kind of value`. `MGET`, the commands about keys, `SET` and `MSET` never say this.
- An argument or a stored string that should be an integer and is not: `ERR value is not an integer or out of range`. An integer is an optional `-` then digits, within `i64`.

**Replies** below are `Frame` values. "Nil" is `Null`. `OK` is `Simple("OK")`. Strings you return are `Bulk`.

| Command | Reply |
|---|---|
| `PING [message]` | `Simple("PONG")`, or the message |
| `ECHO message` | the message |
| `SET key value` | `OK`; replaces any kind |
| `GET key` | the string, or nil |
| `APPEND key value` | new length; a missing key starts empty |
| `STRLEN key` | length, 0 if missing |
| `INCR` / `DECR key`, `INCRBY key n` | the new number; missing counts as 0; if it does not fit: `ERR increment or decrement would overflow` |
| `MSET key value [key value ...]` | `OK`; an odd pair is a wrong number of arguments |
| `MGET key [key ...]` | array, nil for a missing key or another kind |
| `DEL key [key ...]` | how many keys were removed |
| `EXISTS key [key ...]` | how many arguments name a key; a repeat counts again |
| `TYPE key` | `Simple`: `none`, `string`, `list`, `hash` or `set` |
| `RENAME from to` | `OK`; `to` is replaced (renaming a key to itself keeps it); a missing `from` is `ERR no such key` |
| `DBSIZE` | how many keys |
| `FLUSHALL` | `OK`; removes every key |

**Lists.** `LPUSH` / `RPUSH key value [value ...]` add one value at a time at the left or right end, and reply with the length. `LLEN key` is the length. `LRANGE key start stop` is the elements from `start` to `stop`, both included.

`LPOP` / `RPOP key` reply with the removed element, or nil. With a count, `LPOP key n` replies with an array of up to `n` elements in the order removed, or `NullArray` for a missing key. A negative count is `ERR value is out of range, must be positive`. The count is checked before the key. The count comes from a stranger: every count up to `9223372036854775807` is valid, however short the list. Do not reserve memory or loop by the count; the work depends only on what the list holds.

In `LRANGE` and `LTRIM key start stop`, a negative index counts from the end (-1 is the last). Indexes past either end are clamped, and a range can be empty. `LTRIM` keeps only the range and replies `OK`.

**Hashes.** `HSET key field value [field value ...]` replies with the number of new fields; a field with no value is a wrong number of arguments.

`HGET` is the value or nil. `HDEL key field [field ...]` replies with how many it removed. `HLEN` counts fields. `HGETALL` is `field, value, ...` ordered by field.

`HINCRBY key field n` works as `INCRBY` on one field. A stored value that is not an integer is `ERR hash value is not an integer`.

**Sets.** `SADD` / `SREM key member [member ...]` reply with how many were added or removed. `SISMEMBER` is 1 or 0, `SCARD` counts, `SMEMBERS` lists in byte order.

`SINTER`, `SUNION` and `SDIFF key [key ...]` reply with the result in byte order. A missing key is an empty set, and **every** key is checked for its kind. `SDIFF` is the first set without the others.

**A key never holds an empty list, hash or set.** Removing the last element removes the key. Commands that only read or remove never create a key.
