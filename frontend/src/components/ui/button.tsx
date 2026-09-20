import * as React from 'react'
import { Slot } from 'radix-ui'
import { cva, type VariantProps } from 'class-variance-authority'

import { cn } from '@/lib/utils'

const buttonVariants = cva(
  "inline-flex items-center justify-center gap-2 whitespace-nowrap rounded-md text-sm font-medium transition-all disabled:pointer-events-none disabled:opacity-50 [&_svg]:pointer-events-none [&_svg:not([class*='size-'])]:size-4 shrink-0 [&_svg]:shrink-0 outline-none focus-visible:border-ring focus-visible:ring-ring/50 focus-visible:ring-[3px] aria-invalid:ring-destructive/20 aria-invalid:border-destructive",
  {
    variants: {
      variant: {
        default: 'bg-primary text-primary-foreground shadow-xs hover:bg-primary/90',
        destructive:
          'bg-destructive text-white shadow-xs hover:bg-destructive/90 focus-visible:ring-destructive/20',
        outline:
          'border bg-background shadow-xs hover:bg-accent hover:text-accent-foreground dark:bg-input/30 dark:border-input dark:hover:bg-input/50',
        secondary: 'bg-secondary text-secondary-foreground shadow-xs hover:bg-secondary/80',
        ghost: 'hover:bg-accent hover:text-accent-foreground dark:hover:bg-accent/50',
        link: 'text-primary underline-offset-4 hover:underline',
      },
      // Every size grows to the 44px a thumb needs where the pointer is
      // coarse, and stays exactly as it was where it is not. A phone-sized
      // browser window on a desktop is still a mouse; a laptop with a
      // touchscreen is not, and this reads the pointer rather than the width
      // because that is the thing that actually decides how hard a control is
      // to hit. Sized here rather than at each call site so a new button is
      // tappable by default instead of by whoever remembered.
      size: {
        default: 'h-9 px-4 py-2 has-[>svg]:px-3 pointer-coarse:h-11',
        sm: 'h-8 rounded-md gap-1.5 px-3 has-[>svg]:px-2.5 pointer-coarse:h-11',
        lg: 'h-10 rounded-md px-6 has-[>svg]:px-4 pointer-coarse:h-11',
        icon: 'size-9 pointer-coarse:size-11',
        'icon-sm': 'size-7 rounded-md pointer-coarse:size-11',
      },
    },
    defaultVariants: { variant: 'default', size: 'default' },
  },
)

function Button({
  className,
  variant,
  size,
  asChild = false,
  ...props
}: React.ComponentProps<'button'> & VariantProps<typeof buttonVariants> & { asChild?: boolean }) {
  const Comp = asChild ? Slot.Root : 'button'
  return (
    <Comp
      data-slot="button"
      className={cn(buttonVariants({ variant, size, className }))}
      {...props}
    />
  )
}

export { Button, buttonVariants }
