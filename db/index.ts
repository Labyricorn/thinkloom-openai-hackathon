import { drizzle } from "drizzle-orm/d1";
import * as schema from "./schema";

/** Create a typed database client from the D1 binding supplied by the worker. */
export function getDb(binding: D1Database) {
  return drizzle(binding, { schema });
}
