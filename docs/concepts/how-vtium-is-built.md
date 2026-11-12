# How VTium is built

vtium has one maintainer. The maintainer decides what vtium does and judges whether it does it. AI coding agents do most of the writing: the code, the tests, the tools, and the docs. This page explains how that works and why the result can be trusted. [AGENTS.md](https://github.com/vtium/vtium/blob/main/AGENTS.md) describes the working loop that the agents follow. [CONTRIBUTING.md](https://github.com/vtium/vtium/blob/main/CONTRIBUTING.md) says what a contribution must show. This page gives the reasons behind both.

## The idea

Any program worth using is more than one person can hold in their head, just as nobody can memorize a whole city. So the project has to be easy to find your way around. Whoever works on one part of it, a person or an agent, finds that part and its context from where they stand, without knowing the rest.

vtium keeps its knowledge where it can be found when it is needed:

- in the decision records
- in each crate's README
- in AGENTS.md
- in tests tagged with the id of the mode or feature they cover
- in goldens, the saved expected results, kept beside the screens they judge
- in guidance that reaches an agent at the moment it starts a kind of work
- in this section of the docs, one concept to a page

Agents can do dependable work on vtium because the project tells them where they are and what judges their work. Nobody has to hold the whole program in mind. With enough machine time and money, the same structure lets one person maintain vtium without spending all of their time on it. The agents carry the labour. The person keeps the decisions and the judgement, and does not hand those to an agent.

An agent writes code quickly, and the code usually looks right. A change from an agent is a claim until something the agent did not write has checked it. So vtium puts its effort into the checks, and lets the agents put theirs into the code.

## The judge comes first

vtium's unit tests arrived with its first code. The tiers that judge the whole terminal followed right after. They are:

- byte streams to replay, each with the screens and replies it must produce
- images that the renderer must draw exactly, pixel for pixel
- a virtual machine that boots vtium on a VT and captures its screen

Each tier compares vtium's output against its goldens. [How vtium is judged](how-vtium-is-judged.md) describes the tiers and the goldens that belong to them.

The author of a change never rewrites the golden that judges it. A golden changes only in a commit of its own. The maintainer reviews that commit by looking at the old result, the new one, and the difference between them. Without that rule, an agent that cannot make its output match a golden can make the golden match its output instead, and every test passes.

## A test must be seen to fail

Each new test runs twice: before the change, where it fails, and after the change, where it passes. A reviewer also plants the defect that the change claims to catch. The reviewer plants it on a copy of the file, watches the test fail, and puts the file back. If a test stays green with the defect planted, it goes back to its author.

## The record comes first

A decision that sets direction is written down before the code, as a decision record. The record says what is decided, what the decision trades away, and what else was considered. The maintainer reads the record before it merges. An agent then reads the code against the record, statement by statement, and lists where the two differ. That list is the work. The records are under [Decisions](../architecture/INDEX.md).

Agents prepare decisions, and the maintainer makes them. The steering session is the agent that the maintainer works with. For each open question, the steering session sets out the case for and against, says which way it leans, and asks the maintainer. The record states the answer in plain words.

## One person steers, and agents work in parallel

The steering session plans the work, writes the briefs, and merges the results. It runs agents in waves, several at a time. Each agent works in its own git worktree, a separate checkout of the same repository. Each agent also has its own build directory, its own scratch directory, and its own files. No two agents edit the same file. When an agent needs a change to a file that another agent owns, it asks the steering session, which routes the change.

The author of a change commits on its branch and runs the gates. The gates are the checks every change must pass: formatting, lints, the tests, the smallest build, the record checks, and the first two tiers. The author does not push.

A fresh reviewer then reads the diff. A fresh reviewer is a different agent, one that did not write the change. It runs the gates again and plants defects. The author gets two rounds to fix what the reviewer finds. After that, a newly briefed agent takes the work over.

Reviewed branches merge into a branch called integration. The gates run again on the merged tree, because two changes that each pass on their own can fail together.

At a merge point, the steering session picks a commit of integration that passed the gates. It moves a branch called stage to that commit, and opens one pull request from stage to main. The maintainer reads a diff that does not move, while new work goes on in integration. The pull request merges only when every CI job passes, including the one that runs on ARM hardware.

## A report is a claim

An agent's account of what it did is a claim, however confident it sounds. The steering session reads the diff and runs the gates itself before it merges. A reviewer runs the gates instead of trusting the author's results.

## The rules are written where every agent reads them

Every agent starts from the same contract, AGENTS.md. A new author, a fresh reviewer, and a session that has lost its context all start there. Many of its rules exist because an agent once got something wrong, and a gate or a reviewer caught it. Each catch became a rule. Here are three of them:

- A delegated agent once ran its gates as a background job. The notice that the job had ended went to the steering session instead of to the agent. So agents now run every gate in the foreground.
- A test used a stand-in for a real part, and the stand-in accepted a name that the real part refuses. So the defect surfaced only in the virtual machine.
- A test of a console switch once waited for the screen's pixels to stop changing. The maintainer asked for the switch to be held as state, and checked right after the keys. So the rules now forbid fixing or testing with timing.

The guidance that reaches an agent as it starts a kind of work comes from [agent-ways](https://github.com/aaronsb/agent-ways), a tool the maintainer uses. With it, an agent does not have to remember one long file that it read at the start.

## What the maintainer does

The maintainer does four things:

- decides direction and answers the decision records
- reviews every golden that changes
- checks builds by eye on a real VT, with a guide that walks through each step. See [The VT review](../vt-review.md).
- reads each batch on stage before it reaches main

## The limits and the costs

- Agents make mistakes, and some of them look plausible. The gates and the fresh reviewers catch most of them, but not all.
- The goldens judge only what they cover. Nobody judges behaviour that no test covers, until someone writes the test. The table of the modes and features vtium supports lists the tests that cover each one, so it shows where a mode or feature has no test.
- When the maintainer approves a golden by mistake, the mistake becomes the expected result. Every later run passes against it until someone notices.
- The checks cost machine time and money. Each change runs the gates at least three times: for the author, for the reviewer, and on the merged tree. Tier 3 boots virtual machines. Every agent is paid for.
- The maintainer's attention is the part that does not scale. The process spends it on the decisions, the goldens and the screen.

## How the Linux kernel treats AI tools

The Linux kernel has guidance for AI coding assistants, which first shipped with Linux 7.0. It leaves the review of AI-generated code, and the responsibility for it, with the human who submits it. See [AI Coding Assistants](https://docs.kernel.org/process/coding-assistants.html). vtium's CONTRIBUTING.md asks the same of a contributor.

Linus Torvalds has spoken about vibe coding, which is building a program by prompting an AI tool without reading what it writes. He said it is a good way for people to get a computer to do something they could not do otherwise. He also said it is a bad idea, from a maintenance standpoint, for production code. See [The Register, November 2025](https://www.theregister.com/2025/11/18/linus_torvalds_vibe_coding/).

He has also said that he finds the tools useful, and that they cause maintainers real pain as well. He added that a person who uses them still has to understand the result to maintain it in the long run. See [LWN, May 2026](https://lwn.net/Articles/1073761/).
