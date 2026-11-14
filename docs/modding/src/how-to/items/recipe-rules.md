# Reading recipe rules

Use `Host::recipes_of_type` to read loaded recipe definitions and
`LevelRef::recipe_for` to compute the output and remainders for actual inputs.

| Recipe kind | Definition |
|---|---|
| Shaped, shapeless and cooking | Resolved ingredients and fixed results |
| Stonecutting | `RecipeRule::Stonecutting`, with its resolved input and complete fixed output |
| Smithing and special crafting | A typed `RecipeRule` with resolved ingredient roles; the result depends on the inputs |
| Mod-defined type | Original recipe data in `Recipe::json`; match it in your mod |

For smithing and special crafting, do not treat a `RecipeTemplate` as a fixed
stack. It retains the count and component patch used to assemble the result.
In `RecipeRule::SmithingTransform`, absent template or addition ingredients
require empty slots. Trim patterns are registry references or inline definitions.

`recipes_of_type` returns recipes after pack overrides, conditions and filters,
in file order. Read them again after a data reload. On the server, you read the
server's table. A client reads vanilla and content-mod recipes unless the server
sends the recipe type through `datapack_sync`.

One malformed recipe of a known type refuses the **whole recipe load**, with an
error naming its file and the reason. A failed reload keeps the previous active
recipes and does not publish a partial table; a dedicated server stops startup
with that error. A file whose load conditions are false is skipped quietly before
its recipe type is looked up. An unregistered condition type in a registered
namespace is a load error; foreign-namespace condition handling is unchanged.
Unregistered recipe types retain their existing behavior pending owner registration.

`LevelRef::recipe_for` returns `Ok(None)` when no recipe matches. Mod-defined
recipe types return `RecipeLookupError::NotMatchable`; use their JSON to build
your own matching logic. See [Cache reload-stable recipe definitions](reload-stable-recipes.md)
for a custom-type cache.
