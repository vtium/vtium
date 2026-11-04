# How VTium is built

vtium has one maintainer, who decides what vtium does and judges whether it does it. AI coding agents do most of the writing: the code, the tests, the tools and the docs. This page explains how that works and why its output can be trusted. [AGENTS.md](https://github.com/vtium/vtium/blob/main/AGENTS.md) is the working loop the agents follow, and [CONTRIBUTING.md](https://github.com/vtium/vtium/blob/main/CONTRIBUTING.md) says what a contribution must show. This page gives the reasons behind both.

## The idea

Any program worth using is more than one person can hold in their head, the way nobody can memorize a whole city. So the project has to be easy to find your way around. Whoever works on one part of it, a person or an agent, finds that part and its context from where they stand, without knowing the rest. vtium keeps its knowledge where it can be found when it is needed: in the decision records, in each crate's README, in AGENTS.md, in tests tagged to the rows of ADR-1000 they cover, in goldens kept beside the screens they judge, in guidance that reaches an agent at the moment it starts a kind of work, and in this section of the docs, one concept to a page.

Agents can do dependable work on vtium because the project tells them where they are and what judges them. Nobody has to hold the whole program in mind. With enough machine time and money, the same structure lets one person maintain vtium without spending all of their time on it. The agents carry the labour. The person keeps the decisions and the judgement, and does not hand those to an agent.

An agent writes code quickly, and the code usually looks right. A change from an agent is a claim until something the agent did not write has checked it. So vtium puts its effort into the checks, and lets the agents put theirs into the code.

## The judge comes first

vtium's unit tests arrived with its first code, and the tiers that judge the whole terminal followed right after it: byte streams to replay with the screens and replies they must produce, pictures the renderer must draw pixel for pixel, and a virtual machine that boots vtium on a VT and captures its screen. The saved expected results are called goldens, and [How vtium is judged](how-vtium-is-judged.md) describes the tiers they belong to.

The author of a change never rewrites the golden that judges it, and a golden changes only in a commit of its own that the maintainer reviews by looking at the old result, the new one and the difference. Without that rule, an agent that cannot make its output match a golden can make the golden match its output, and every test passes.

## A test must be seen to fail

Each new test is run before the change, where it fails, and after it, where it passes. A reviewer also plants the defect the change claims to catch, on a copy of the file, watches the test fail, and puts the file back. A test that stays green with the defect planted goes back to its author.

## The record comes first

A decision that sets direction is written down before the code, as a decision record: what is decided, what it trades away, and what else was considered. The maintainer reads it before it merges. An agent then reads the code against the record, statement by statement, and lists where they differ, and that list is the work. The records are under [Decisions](../architecture/INDEX.md).

Agents prepare decisions and the maintainer makes them. For each open question, the steering session, the agent the maintainer works with, sets out the case for and against, says which way it leans, and asks. The record states the answer in plain words.

## One person steers, and agents work in parallel

The steering session plans the work, writes the briefs and merges. It runs agents in waves, several at a time. Each agent works in its own git worktree, with its own build directory, its own scratch directory and its own files, and no two agents edit the same file. An agent that needs a file another owns asks the steering session, which routes the change.

The author of a change commits on its branch and runs the gates, the checks every change must pass: formatting, lints, the tests, the smallest build, the record checks, and the first two tiers. The author does not push. A fresh reviewer, a different agent that did not write the change, reads the diff, runs the gates again and plants defects. The author gets two rounds to fix what the reviewer finds, and after that a newly briefed agent takes the work over.

Reviewed branches merge into a branch called integration, and the gates run again on the merged tree, since two changes that each pass can fail together. At a merge point the steering session picks a commit of integration that passed the gates, moves a branch called stage to it, and opens one pull request from stage to main. The maintainer reads a diff that does not move while new work goes on in integration. The pull request merges only when every CI job passes, including the one that runs on ARM hardware.

## A report is a claim

An agent's account of what it did is a claim, however confident it sounds. The steering session reads the diff and runs the gates itself before it merges. A reviewer runs the gates instead of trusting the author's results.

## The rules are written where every agent reads them

Every agent starts from the same contract, AGENTS.md, whether it is a new author, a fresh reviewer or a session that has lost its context. Many of its rules exist because an agent once got something wrong and a gate or a reviewer caught it: a delegated agent once ran its gates as a background job, whose notice went to the steering session instead of to the agent, so agents now run every gate in the foreground; a test's stand-in for a real part accepted a name the real part refuses, so the defect surfaced only in the virtual machine; a test of a console switch once waited for the screen's pixels to stop changing, and the maintainer asked for the switch to be held as state and checked right after the keys, so the rules now forbid fixing or testing with timing. Each catch became a rule. The guidance that reaches an agent as it starts a kind of work comes from [agent-ways](https://github.com/aaronsb/agent-ways), a tool the maintainer uses, so an agent does not have to remember one long file it read at the start.

## What the maintainer does

The maintainer decides direction and answers the decision records, reviews every golden that changes, checks builds by eye on a real VT with a guide that walks through each step ([The VT review](../vt-review.md)), and reads each batch on stage before it reaches main.

## The limits and the costs

- Agents make mistakes, and some of them look plausible. The gates and the fresh reviewers catch most of them, not all.
- The goldens judge only what they cover. Behaviour no test covers is judged by nobody until someone writes the test. ADR-1000's rows list the tests that cover them, which shows where that is the case.
- A golden the maintainer approves by mistake makes the mistake the expected result, and every later run passes against it until someone notices.
- The checks cost machine time and money. Each change runs the gates at least three times, for the author, for the reviewer and on the merged tree, tier 3 boots virtual machines, and every agent is paid for.
- The maintainer's attention is the part that does not scale. The process spends it on the decisions, the goldens and the screen.

## How the Linux kernel treats AI tools

The Linux kernel's guidance for AI coding assistants, first shipped with Linux 7.0, leaves the review of AI-generated code and the responsibility for it with the human who submits it ([AI Coding Assistants](https://docs.kernel.org/process/coding-assistants.html)), and vtium's CONTRIBUTING.md asks the same of a contributor. Linus Torvalds has said that vibe coding, building a program by prompting an AI tool without reading what it writes, is a good way for people to get a computer to do something they could not do otherwise, and a bad idea, from a maintenance standpoint, for production code ([The Register, November 2025](https://www.theregister.com/2025/11/18/linus_torvalds_vibe_coding/)). He has also said that he finds the tools useful, that they cause maintainers real pain as well, and that a person who uses them still has to understand the result to maintain it in the long run ([LWN, May 2026](https://lwn.net/Articles/1073761/)).
