A network is made of servers. Each `Server` has a `name` and a list `links` of the servers it can talk to. Links can form loops, and a server can even link to itself.

`copy_network(start)` takes any server of a network and returns a **deep copy** of the network: the same names and links, but built from brand-new `Server` objects. Return the copy of `start`; return `None` if `start` is `None`.

```python
a = Server("a"); b = Server("b")
a.links = [b]; b.links = [a]      # a loop
copy = copy_network(a)
copy is a                          # False
copy.links[0].links[0] is copy     # True: the loop is copied as a loop
```

Keep the `Server` class as it is in the starter.

A network can be a chain of 5,000 servers. That is deeper than Python's recursion limit (about 1,000 calls), so a recursive copy fails.
