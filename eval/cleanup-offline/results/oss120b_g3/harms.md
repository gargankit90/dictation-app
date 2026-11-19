# oss120b_g3: clips the cleanup broke

Raw Whisper draft judged perfect (0 edits) and the pasted text not, by the judge(s) named. Read each by hand: dropped content, changed meaning, or a judge quirk.

## dict_0346 (check) — broke for claude; reason cleaned

- raw: Ok, so in the FastAPI service, the user search endpoint is doing a full table scan. Can you add a trigram index on the user's table for the name and email columns with the new Alembic migration and then switch the query to use ilike so it actually hits the index. Limit results to 25 per page, don't change the response schema because the mobile app depends on it, and run the integration test after.
- pasted: in the FastAPI service, the user search endpoint is doing a full table scan. Can you add a trigram index on the user's table for the name and email columns with the new Alembic migration and then switch the query to use ilike so it actually hits the index. Limit results to 25 per page, don't change the response schema because the mobile app depends on it, and run the integration test after.
- claude judge's fix of the pasted text: in the FastAPI service, the user search endpoint is doing a full table scan. Can you add a trigram index on the users table for the name and email columns with the new Alembic migration and then switch the query to use ILIKE so it actually hits the index. Limit results to 25 per page, don't change the response schema because the mobile app depends on it, and run the integration test after.

## dict_0350 (check) — broke for claude; reason cleaned

- raw: Ok, so write a Rails migration that adds a nullable archive timestamp to the projects table plus an index on it. And then update the project model so the default scope hides archive projects. Actually no, don't use a default scope. Add a name scope called active instead. We are on Rails 7.1 so use the newer migration syntax and add a model spec for the scope
- pasted: write a Rails migration that adds a nullable archive timestamp to the projects table plus an index on it. then update the project model so the default scope hides archive projects. Actually no, don't use a default scope. Add a name scope called active instead. We are on Rails 7.1 use the newer migration syntax and add a model spec for the scope
- claude judge's fix of the pasted text: write a Rails migration that adds a nullable archive timestamp to the projects table plus an index on it. then update the project model so the default scope hides archive projects. Actually no, don't use a default scope. Add a named scope called active instead. We are on Rails 7.1 use the newer migration syntax and add a model spec for the scope

## dict_0501 (check) — broke for claude; reason cleaned

- raw: Hi, I'm looking at the winter break camp and had some questions before I sign up my son. What time does drop off start in the morning? Can kids bring their own lunch or is it provided? And is there a discount if he only comes three days instead of the whole week? Also do they go outside if it's cold so I know whether to pack snow gear? Thanks.
- pasted: Hi, I'm looking at the winter break camp and had some questions before I sign up my son. What time does drop off start in the morning? Can kids bring their own lunch or is it provided? And is there a discount if he only comes three days instead of the whole week? Also do they go outside if it's cold so I know whether to pack snow gear?
- claude judge's fix of the pasted text: Hi, I'm looking at the winter break camp and had some questions before I sign up my son. What time does drop off start in the morning? Can kids bring their own lunch or is it provided? And is there a discount if he only comes three days instead of the whole week? Also do they go outside if it's cold so I know whether to pack snow gear? Thanks.
