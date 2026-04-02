Deno.serve({ port: 8000 }, async (req) => {
  console.log(await req.json());
  return new Response();
});
