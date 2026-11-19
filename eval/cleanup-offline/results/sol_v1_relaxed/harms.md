# sol_v1_relaxed: clips the cleanup broke

Raw Whisper draft judged perfect (0 edits) and the pasted text not, by the judge(s) named. Read each by hand: dropped content, changed meaning, or a judge quirk.

## dict_0346 (check) — broke for claude; reason cleaned

- raw: Ok, so in the FastAPI service, the user search endpoint is doing a full table scan. Can you add a trigram index on the user's table for the name and email columns with the new Alembic migration and then switch the query to use ilike so it actually hits the index. Limit results to 25 per page, don't change the response schema because the mobile app depends on it, and run the integration test after.
- pasted: in the FastAPI service, the user search endpoint is doing a full table scan. Can you add a trigram index on the user's table for the name and email columns with the new Alembic migration and then switch the query to use ilike so it actually hits the index. Limit results to 25 per page, don't change the response schema because the mobile app depends on it, and run the integration test after.
- claude judge's fix of the pasted text: in the FastAPI service, the user search endpoint is doing a full table scan. Can you add a trigram index on the users table for the name and email columns with the new Alembic migration and then switch the query to use ILIKE so it actually hits the index. Limit results to 25 per page, don't change the response schema because the mobile app depends on it, and run the integration test after.
