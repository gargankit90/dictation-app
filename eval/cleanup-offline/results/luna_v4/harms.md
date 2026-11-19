# luna_v4: clips the cleanup broke

Raw Whisper draft judged perfect (0 edits) and the pasted text not, by the judge(s) named. Read each by hand: dropped content, changed meaning, or a judge quirk.

## dict_0350 (check) — broke for claude; reason cleaned

- raw: Ok, so write a Rails migration that adds a nullable archive timestamp to the projects table plus an index on it. And then update the project model so the default scope hides archive projects. Actually no, don't use a default scope. Add a name scope called active instead. We are on Rails 7.1 so use the newer migration syntax and add a model spec for the scope
- pasted: Write a Rails migration that adds a nullable archive timestamp to the projects table plus an index on it. And then update the project model so the default scope hides archive projects. Actually no, don't use a default scope. Add a name scope called active instead. We are on Rails 7.1 so use the newer migration syntax and add a model spec for the scope
- claude judge's fix of the pasted text: Write a Rails migration that adds a nullable archive timestamp to the projects table plus an index on it. And then update the project model so the default scope hides archive projects. Actually no, don't use a default scope. Add a named scope called active instead. We are on Rails 7.1 so use the newer migration syntax and add a model spec for the scope

## dict_0474 (tune) — broke for claude; reason cleaned

- raw: Hi Carlos, we got invoice number 8841, but it's I think it's showing $2,400 instead of the $2,100 we agreed on. Can you send a corrected one?
- pasted: Hi Carlos, we got invoice number 8841, but it's showing $2,400 instead of the $2,100 we agreed on. Can you send a corrected one?
- claude judge's fix of the pasted text: Hi Carlos, we got invoice number 8841, but I think it's showing $2,400 instead of the $2,100 we agreed on. Can you send a corrected one?
