> **Spoiler before you open the editor.** The starter of this step contains reference code for steps 1 to 5. It is a stand-in, because the app cannot yet carry your own code from one step to the next. It solves those steps and much of the preparation before them, so solve those first. You may replace it with your own code; keep the public names.

**Capstone, step 6 of 9.** Add classes to the tree-walking interpreter.

```text
class Dog < Animal {
  init(name) { super.init(name); this.tricks = list(); }
  speak() { return super.speak() + " (woof)"; }
}
var d = Dog("Rex");
```

**Classes.** A class declaration creates a class value and binds its name. `< Name` names the superclass, which must be a class when the declaration runs: otherwise `Superclass must be a class.`, on that name's line. Methods are inherited. A subclass's method with the same name replaces the inherited one.

**Instances.** Calling a class creates an instance.

- If the class has or inherits a method `init`, it runs on the new instance with the call's arguments, and the usual arity rule.
- Without one, any argument is `Expected 0 arguments but got 2.`
- The call gives the instance. `init` also gives the instance when it is called again directly, even after a bare `return;`.

**Properties.** `a.name` reads a field of the instance if it has one, and otherwise a method of its class. `a.name = v` creates or replaces a field and gives `v`.

- Reading from anything but an instance: `Only instances have properties.`
- Writing: `Only instances have fields.` The target is evaluated and checked **before** the value.
- Neither a field nor a method: `Undefined property 'name'.` These errors are on the line of the property's name.

The order of evaluation stays left to right: operands, then callee before arguments.

**Methods are values.** Reading a method gives a function that remembers its instance: `var m = d.speak; m();` works, wherever it is called from.

**`this` and `super`.** `this` is the instance the method was bound to. Inner functions can use it too.

`super.name` looks for the method starting at the superclass **of the class whose method this text is in**. The instance's own class does not matter.

It is bound to the same `this`. If there is none: `Undefined property 'name'.`

**Printing and comparing.** A class prints as `<class Dog>`, an instance as `Dog instance`, and a method like a function. `type` gives `"class"`, `"instance"` and `"function"`. Classes and instances are equal only to themselves.

**Traces** name a method by its own name: `[line 4] in go()`. Calls to `init` count like any other call, toward the depth limit too.

**Live objects.** Classes, instances and each function value count toward `MAX_LIVE`, including those made when a method is read, until they are freed. Chains through fields or superclasses must end in the memory error, not in a crash.

A chain of superclasses may be tens of thousands long, so finding a method must not use recursion.
