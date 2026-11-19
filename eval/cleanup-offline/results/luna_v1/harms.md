# luna_v1: clips the cleanup broke

Raw Whisper draft judged perfect (0 edits) and the pasted text not, by the judge(s) named. Read each by hand: dropped content, changed meaning, or a judge quirk.

## dict_0235 (check) — broke for claude, codex; reason cleaned

- raw: I coordinate volunteers for a food bank and I need a short guide for first-time volunteers. It should tell them what to wear, where to check in, what a typical shift looks like, and who to ask if they're unsure about something. Write it in plain, friendly language, something I can print on one page and hand out at the door.
- pasted: I coordinate volunteers for a food bank and I need a short guide for first-time volunteers. It should tell them what to wear, where to check in, what a typical shift looks like, and who to ask if they're unsure about something.
- claude judge's fix of the pasted text: I coordinate volunteers for a food bank and I need a short guide for first-time volunteers. It should tell them what to wear, where to check in, what a typical shift looks like, and who to ask if they're unsure about something. Write it in plain, friendly language, something I can print on one page and hand out at the door.
- codex judge's fix of the pasted text: I coordinate volunteers for a food bank and I need a short guide for first-time volunteers. It should tell them what to wear, where to check in, what a typical shift looks like, and who to ask if they're unsure about something. Write it in plain, friendly language, something I can print on one page and hand out at the door.

## dict_0335 (tune) — broke for claude, codex; reason cleaned

- raw: The Go Worker is panicking with a nil map error in the rate limiter. It started with release 2.14, wait 2.15. Can you find which commit caused this?
- pasted: The Go Worker is panicking with a nil map error in the rate limiter. It started with release 2.14, 2.15.
- claude judge's fix of the pasted text: The Go Worker is panicking with a nil map error in the rate limiter. It started with release 2.14, 2.15. Can you find which commit caused it?
- codex judge's fix of the pasted text: The Go Worker is panicking with a nil map error in the rate limiter. It started with release 2.14, 2.15. Can you find which commit caused it?

## dict_0350 (check) — broke for claude; reason cleaned

- raw: Ok, so write a Rails migration that adds a nullable archive timestamp to the projects table plus an index on it. And then update the project model so the default scope hides archive projects. Actually no, don't use a default scope. Add a name scope called active instead. We are on Rails 7.1 so use the newer migration syntax and add a model spec for the scope
- pasted: write a Rails migration that adds a nullable archive timestamp to the projects table plus an index on it. Update the project model. Add a name scope called active instead. We are on Rails 7.1 so use the newer migration syntax and add a model spec for the scope
- claude judge's fix of the pasted text: write a Rails migration that adds a nullable archive timestamp to the projects table plus an index on it. Update the project model so archived projects are hidden, but don't use a default scope. Add a named scope called active instead. We are on Rails 7.1 so use the newer migration syntax and add a model spec for the scope

## dict_0474 (tune) — broke for claude; reason cleaned

- raw: Hi Carlos, we got invoice number 8841, but it's I think it's showing $2,400 instead of the $2,100 we agreed on. Can you send a corrected one?
- pasted: Hi Carlos, we got invoice number 8841, it's showing $2,400 instead of the $2,100 we agreed on. Can you send a corrected one?
- claude judge's fix of the pasted text: Hi Carlos, we got invoice number 8841, but I think it's showing $2,400 instead of the $2,100 we agreed on. Can you send a corrected one?

## dict_0621 (check) — broke for claude, codex; reason cleaned

- raw: Thanks for watering the plants while we're gone. Keys under the blue pot, sorry, the green one, by the back steps.
- pasted: Thanks for watering the plants while we're gone. Keys under the green one, by the back steps.
- claude judge's fix of the pasted text: Thanks for watering the plants while we're gone. Keys under the green pot, by the back steps.
- codex judge's fix of the pasted text: Thanks for watering the plants while we're gone. Keys under the green pot, by the back steps.
