/*
 * Minimal portable subset of BSD <sys/queue.h> (LIST_* and SLIST_* families) so the
 * NativeScript quickjs Node-API shim (written for clang/Android/BSD) compiles under MSVC,
 * which ships no <sys/queue.h>. Definitions are the canonical BSD ones (public domain).
 * Only the macros the shim uses are provided.
 */
#ifndef _COMPAT_SYS_QUEUE_H_
#define _COMPAT_SYS_QUEUE_H_

#ifndef NULL
#include <stddef.h>
#endif

/* ---- Singly-linked list ------------------------------------------------- */
#define SLIST_HEAD(name, type)                                                 \
    struct name {                                                              \
        struct type *slh_first;                                                \
    }
#define SLIST_HEAD_INITIALIZER(head) { NULL }
#define SLIST_ENTRY(type)                                                      \
    struct {                                                                   \
        struct type *sle_next;                                                 \
    }

#define SLIST_FIRST(head)       ((head)->slh_first)
#define SLIST_END(head)         NULL
#define SLIST_EMPTY(head)       (SLIST_FIRST(head) == NULL)
#define SLIST_NEXT(elm, field)  ((elm)->field.sle_next)

#define SLIST_INIT(head)                                                       \
    do {                                                                       \
        SLIST_FIRST(head) = NULL;                                             \
    } while (0)

#define SLIST_INSERT_HEAD(head, elm, field)                                    \
    do {                                                                       \
        (elm)->field.sle_next = (head)->slh_first;                            \
        (head)->slh_first = (elm);                                            \
    } while (0)

#define SLIST_INSERT_AFTER(slistelm, elm, field)                               \
    do {                                                                       \
        (elm)->field.sle_next = (slistelm)->field.sle_next;                   \
        (slistelm)->field.sle_next = (elm);                                   \
    } while (0)

#define SLIST_REMOVE_HEAD(head, field)                                         \
    do {                                                                       \
        (head)->slh_first = (head)->slh_first->field.sle_next;                \
    } while (0)

#define SLIST_REMOVE(head, elm, type, field)                                   \
    do {                                                                       \
        if ((head)->slh_first == (elm)) {                                     \
            SLIST_REMOVE_HEAD((head), field);                                 \
        } else {                                                               \
            struct type *curelm = (head)->slh_first;                          \
            while (curelm->field.sle_next != (elm))                           \
                curelm = curelm->field.sle_next;                             \
            curelm->field.sle_next = curelm->field.sle_next->field.sle_next;  \
        }                                                                      \
    } while (0)

#define SLIST_FOREACH(var, head, field)                                        \
    for ((var) = SLIST_FIRST(head); (var); (var) = SLIST_NEXT(var, field))

/* ---- Doubly-linked list ------------------------------------------------- */
#define LIST_HEAD(name, type)                                                  \
    struct name {                                                              \
        struct type *lh_first;                                                 \
    }
#define LIST_HEAD_INITIALIZER(head) { NULL }
#define LIST_ENTRY(type)                                                       \
    struct {                                                                   \
        struct type *le_next;                                                  \
        struct type **le_prev;                                                 \
    }

#define LIST_FIRST(head)        ((head)->lh_first)
#define LIST_END(head)          NULL
#define LIST_EMPTY(head)        (LIST_FIRST(head) == NULL)
#define LIST_NEXT(elm, field)   ((elm)->field.le_next)

#define LIST_INIT(head)                                                        \
    do {                                                                       \
        LIST_FIRST(head) = NULL;                                              \
    } while (0)

#define LIST_INSERT_HEAD(head, elm, field)                                     \
    do {                                                                       \
        if (((elm)->field.le_next = (head)->lh_first) != NULL)               \
            (head)->lh_first->field.le_prev = &(elm)->field.le_next;          \
        (head)->lh_first = (elm);                                             \
        (elm)->field.le_prev = &(head)->lh_first;                             \
    } while (0)

#define LIST_INSERT_AFTER(listelm, elm, field)                                 \
    do {                                                                       \
        if (((elm)->field.le_next = (listelm)->field.le_next) != NULL)       \
            (listelm)->field.le_next->field.le_prev = &(elm)->field.le_next;  \
        (listelm)->field.le_next = (elm);                                     \
        (elm)->field.le_prev = &(listelm)->field.le_next;                     \
    } while (0)

#define LIST_INSERT_BEFORE(listelm, elm, field)                                \
    do {                                                                       \
        (elm)->field.le_prev = (listelm)->field.le_prev;                      \
        (elm)->field.le_next = (listelm);                                     \
        *(listelm)->field.le_prev = (elm);                                    \
        (listelm)->field.le_prev = &(elm)->field.le_next;                     \
    } while (0)

#define LIST_REMOVE(elm, field)                                                \
    do {                                                                       \
        if ((elm)->field.le_next != NULL)                                    \
            (elm)->field.le_next->field.le_prev = (elm)->field.le_prev;       \
        *(elm)->field.le_prev = (elm)->field.le_next;                         \
    } while (0)

#define LIST_FOREACH(var, head, field)                                         \
    for ((var) = LIST_FIRST(head); (var); (var) = LIST_NEXT(var, field))

#endif /* _COMPAT_SYS_QUEUE_H_ */
