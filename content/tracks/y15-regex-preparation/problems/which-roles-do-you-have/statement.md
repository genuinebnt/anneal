In an access system, a role can include other roles. Some inclusions only apply at certain times: after a second login factor, or from the office network.

Write `access(links, held, context)`. `links[r]` lists what role `r` includes. `held` lists the roles a user was given directly. Roles are numbered from 0, and every number used is valid.

A link is **open** when its condition holds in `context`: `Always` is always open, `Mfa` needs `context.mfa`, and `OnSite` needs `context.on_site`.

Return:

- `roles`: every role the user has: those held, and every role reached from them through any number of open links.
- `blocked`: every role the user does **not** have, but which is the target of a closed link from a role the user has. This is what they would be told they are missing by one step.

Both lists are sorted and name each role once. Inclusions may form loops. `held` may repeat a role.

**Sizes.** There can be hundreds of thousands of roles, in chains as long as that. The number of different paths between two roles can be astronomically large. `held` can be as long as the list of roles.
