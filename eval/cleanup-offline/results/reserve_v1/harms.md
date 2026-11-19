# reserve_v1: clips the cleanup broke

Raw Whisper draft judged perfect (0 edits) and the pasted text not, by the judge(s) named. Read each by hand: dropped content, changed meaning, or a judge quirk.

## dict_0295 (tune) — broke for claude; reason cleaned

- raw: Also update the README with the new command.
- pasted: Update the README with the new command.
- claude judge's fix of the pasted text: Also update the README with the new command.

## dict_0346 (check) — broke for claude; reason cleaned

- raw: Ok, so in the FastAPI service, the user search endpoint is doing a full table scan. Can you add a trigram index on the user's table for the name and email columns with the new Alembic migration and then switch the query to use ilike so it actually hits the index. Limit results to 25 per page, don't change the response schema because the mobile app depends on it, and run the integration test after.
- pasted: in the FastAPI service, the user search endpoint is doing a full table scan. Can you add a trigram index on the user's table for the name and email columns with the new Alembic migration and then switch the query to use ilike so it actually hits the index. Limit results to 25 per page, don't change the response schema because the mobile app depends on it, and run the integration test after.
- claude judge's fix of the pasted text: in the FastAPI service, the user search endpoint is doing a full table scan. Can you add a trigram index on the users table for the name and email columns with the new Alembic migration and then switch the query to use ILIKE so it actually hits the index. Limit results to 25 per page, don't change the response schema because the mobile app depends on it, and run the integration test after.

## dict_0350 (check) — broke for claude; reason cleaned

- raw: Ok, so write a Rails migration that adds a nullable archive timestamp to the projects table plus an index on it. And then update the project model so the default scope hides archive projects. Actually no, don't use a default scope. Add a name scope called active instead. We are on Rails 7.1 so use the newer migration syntax and add a model spec for the scope
- pasted: write a Rails migration that adds a nullable archive timestamp to the projects table plus an index on it. And then update the project model so the default scope hides archive projects. Actually no, don't use a default scope. Add a name scope called active instead. We are on Rails 7.1 so use the newer migration syntax and add a model spec for the scope
- claude judge's fix of the pasted text: write a Rails migration that adds a nullable archive timestamp to the projects table plus an index on it. And then update the project model so the default scope hides archive projects. Actually no, don't use a default scope. Add a named scope called active instead. We are on Rails 7.1 so use the newer migration syntax and add a model spec for the scope

## dict_0414 (tune) — broke for claude, codex; reason cleaned

- raw: Passport renewal plan before the trip. Find the old passports first. I think they're in the fire safe. Then get new photos done at the pharmacy. Fill out the renewal forms online and print them and mail everything with the check with tracking. And set a reminder to check the status in a few weeks so we're not cutting it close.
- pasted: Passport renewal plan before the trip. Find the old passports first. They're in the fire safe. Then get new photos done at the pharmacy. Fill out the renewal forms online and print them and mail everything with the check with tracking. Set a reminder to check the status in a few weeks so we're not cutting it close.
- claude judge's fix of the pasted text: Passport renewal plan before the trip. Find the old passports first. I think they're in the fire safe. Then get new photos done at the pharmacy. Fill out the renewal forms online and print them and mail everything with the check with tracking. Set a reminder to check the status in a few weeks so we're not cutting it close.
- codex judge's fix of the pasted text: Passport renewal plan before the trip. Find the old passports first. I think they're in the fire safe. Then get new photos done at the pharmacy. Fill out the renewal forms online and print them and mail everything with the check with tracking. Set a reminder to check the status in a few weeks so we're not cutting it close.

## dict_0474 (tune) — broke for claude; reason cleaned

- raw: Hi Carlos, we got invoice number 8841, but it's I think it's showing $2,400 instead of the $2,100 we agreed on. Can you send a corrected one?
- pasted: Hi Carlos, we got invoice number 8841, but it's showing $2,400 instead of the $2,100 we agreed on. Can you send a corrected one?
- claude judge's fix of the pasted text: Hi Carlos, we got invoice number 8841, but I think it's showing $2,400 instead of the $2,100 we agreed on. Can you send a corrected one?

## dict_0506 (check) — broke for claude, codex; reason cleaned

- raw: Hi Priya, just confirming our walkthrough at the Fremont property Tuesday the 12th at 10 a.m. Sorry, 1030. The electrician can't get there before 10. Address is 4218 Mowry Avenue and you can park in the side lot. If anything changes, my cell is 415-555-0147. See you then, Dana.
- pasted: Hi Priya, just confirming our walkthrough at the Fremont property Tuesday the 12th at 1030. The electrician can't get there before 10. Address is 4218 Mowry Avenue and you can park in the side lot. If anything changes, my cell is 415-555-0147. See you then, Dana.
- claude judge's fix of the pasted text: Hi Priya, just confirming our walkthrough at the Fremont property Tuesday the 12th at 10:30 AM. The electrician can't get there before 10. Address is 4218 Mowry Avenue and you can park in the side lot. If anything changes, my cell is 415-555-0147. See you then, Dana.
- codex judge's fix of the pasted text: Hi Priya, just confirming our walkthrough at the Fremont property Tuesday the 12th at 1030 AM. The electrician can't get there before 10. Address is 4218 Mowry Avenue and you can park in the side lot. If anything changes, my cell is 415-555-0147. See you then, Dana.

## dict_0621 (check) — broke for claude, codex; reason cleaned

- raw: Thanks for watering the plants while we're gone. Keys under the blue pot, sorry, the green one, by the back steps.
- pasted: Thanks for watering the plants while we're gone. Keys under the green one, by the back steps.
- claude judge's fix of the pasted text: Thanks for watering the plants while we're gone. Keys under the green pot, by the back steps.
- codex judge's fix of the pasted text: Thanks for watering the plants while we're gone. Keys under the green pot, by the back steps.
