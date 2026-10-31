/* MuPDF builds HarfBuzz so that every allocation goes through fz_hb_malloc.
   Those functions read a context stored by fz_hb_lock. The viewer calls hb_*
   directly, without that lock, and the null context is a segmentation fault
   on the first label (right after the framebuffer resolution line).
   libc does not need the context. Replacing the six symbols keeps MuPDF's
   own harfbuzz.o out of the link. */
#include <stdlib.h>

void *fz_hb_malloc(size_t size)
{
	return malloc(size ? size : 1);
}

void *fz_hb_calloc(size_t n, size_t size)
{
	return calloc(n ? n : 1, size ? size : 1);
}

void *fz_hb_realloc(void *ptr, size_t size)
{
	return realloc(ptr, size ? size : 1);
}

void fz_hb_free(void *ptr)
{
	free(ptr);
}

void fz_hb_lock(void *ctx)
{
	(void)ctx;
}

void fz_hb_unlock(void *ctx)
{
	(void)ctx;
}
