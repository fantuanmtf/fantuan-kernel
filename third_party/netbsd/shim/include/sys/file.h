/* fantuan adaptation shim: sys/file.h - compile-only description of the
 * kernel file object for uipc_socket.c (ours, not upstream).  The VFS/fd
 * layer is outside the M11 slice: the kernel socket client never allocates
 * a struct file; fsocreate() is compiled but unreachable. */
#ifndef FANTUAN_SYS_FILE_H
#define FANTUAN_SYS_FILE_H

#include <sys/types.h>

#define FREAD		0x0001
#define FWRITE		0x0002
#define FNONBLOCK	0x0004
#define FNOSIGPIPE	0x0008

#define O_CLOEXEC	0x00400000	/* NetBSD sys/fcntl.h value */

#define DTYPE_SOCKET	3

struct file;
struct uio;
struct stat;
struct knote;
struct uvm_object;
struct socket;

struct fileops {
	const char *fo_name;
	int	(*fo_read)(struct file *, off_t *, struct uio *, void *, int);
	int	(*fo_write)(struct file *, off_t *, struct uio *, void *, int);
	int	(*fo_ioctl)(struct file *, u_long, void *);
	int	(*fo_fcntl)(struct file *, u_int, void *);
	int	(*fo_poll)(struct file *, int);
	int	(*fo_stat)(struct file *, struct stat *);
	int	(*fo_close)(struct file *);
	int	(*fo_kqfilter)(struct file *, struct knote *);
	void	(*fo_restart)(struct file *);
	int	(*fo_mmap)(struct file *, off_t *, size_t, int, int *, int *,
		    struct uvm_object **, int *);
	int	(*fo_seek)(struct file *, off_t, int, off_t *, int);
};

typedef struct file file_t;

struct file {
	int	f_flag;		/* file flag */
	int	f_type;		/* descriptor type */
	const struct fileops *f_ops;
	struct socket *f_socket;
	void	*f_data;
};

#endif
