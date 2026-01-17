---
description: Guidelines for creating a new feature
globs:
alwaysApply: false
---

## guidelines when planning a new feature
Here are a list of guidelines that should be followed when tasked with adding a feature. 
1. When planning, explore the existing code before making any decisions - the new feature should fit the existing code architecture without any hacks our ugly patches. If it doesn't
   and requires a big architectural change fully explain your reasoning to the user, and ask what we want to include in this feature and what it out of scope.
2. When planning, as you design the set of tasks to do, weave in testing as part of doing the test. Don't just generate a task to do the test at the end; weave it so with each test you generate the regression tests, and kind of do it like a little tdd as you go along.
3. Co-create with me. This means that when you're doing a plan, I want you to interview me and ask me a ton of questions, so every decision that you're not sure about, just ask. The more questions that you ask, the more ideal the plan would be. And you need to research carefully. Even if as you go, you can you know ask me a few questions, then go up and look some sources and then ask me more questions. It can be like an iterative process.
4. in your plan make sure to include code examples for new code you want to generate and reference the existing code you will be changing.