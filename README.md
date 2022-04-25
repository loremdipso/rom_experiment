# Explanation

File systems are general purpose. They need to be resilient to failures, support read and write, handle concurrent access, etc. This is great, and many extremely talented people have worked tirelessly to make the file systems of today unbelievably fast. Can we do better?

Just because a file system _can_ do anything doesn't mean we always need it to. A common pattern in game development is to group your read-only files into a smaller number of tar-esque files. The idea being that you can do really really really fast reads from this bundle, and if your files are small enough you might even save some page-faults when fetching files that you know in advance will be accessed together.

But we've made so many advancements over the years. Is this bundling even still worth it? Let's find out.

# Initial Results

Just from playing around with some parameters it seems as though this can give you a roughly 2x speedup in reads (!!!), but only for small files. As the files get larger the time it takes to read them from our tar file or directly from disk begin to look identical, but reading from the archive is often still marginally faster.

# Premature conclusions

I think this can be worth the effort if you're loading many small files. Obviously you'd want to try this out on your own set of files and target platforms to really see.
