//! An IDO pool program implementing the Mango Markets token sale design here:
//! https://docs.mango.markets/litepaper#token-sale.

use anchor_lang::prelude::*;
use anchor_lang::solana_program::program_option::COption;
use anchor_spl::token::{self, Token, Burn, Mint, MintTo, TokenAccount, Transfer};
use std::cmp;
use std::convert::TryInto;
use std::str::FromStr;

#[cfg(feature = "local-testing")]
declare_id!("3zSwHpEF8svwihadvnx7q2EagTyW1tvwn354gzvF5Zh4");

#[cfg(not(feature = "local-testing"))]
declare_id!("3zSwHpEF8svwihadvnx7q2EagTyW1tvwn354gzvF5Zh4");

#[cfg(feature = "local-testing")]
const ALLOWED_DEPLOYER: &str = "52ANpnRU92jw3gnE1ut2nPhhmmcm5ThvjXMbScU7Yys9";

#[cfg(not(feature = "local-testing"))]
const ALLOWED_DEPLOYER: &str = "3FadrT6JsE5GSrLFUy4qPvA26EMBzAHuG5uvYWcCWVCa";

#[program]
pub mod ido_pool {
    use super::*;

    #[access_control(InitializePool::accounts(&ctx, nonce) future_start_time(start_ido_ts))]
    pub fn initialize_pool(
        ctx: Context<InitializePool>,
        num_ido_tokens: u64,
        max_usdc_tokens: u64,
        nonce: u8,
        start_ido_ts: i64,
        end_ido_ts: i64,
        withdraw_melon_ts: i64,
    ) -> Result<()> {
        if !(start_ido_ts < end_ido_ts && end_ido_ts < withdraw_melon_ts) {
            return Err(ErrorCode::SeqTimes.into());
        }
        if num_ido_tokens == 0 || max_usdc_tokens == 0 {
            return Err(ErrorCode::InvalidParam.into());
        }

        let pool_account = &mut ctx.accounts.pool_account;
        if Pubkey::from_str(ALLOWED_DEPLOYER).unwrap() != *ctx.accounts.payer.to_account_info().key
        {
            return Err(ErrorCode::InvalidParam.into());
        }
        pool_account.redeemable_mint = *ctx.accounts.redeemable_mint.to_account_info().key;
        pool_account.pool_watermelon = *ctx.accounts.pool_watermelon.to_account_info().key;
        pool_account.watermelon_mint = ctx.accounts.pool_watermelon.mint;
        pool_account.pool_usdc = *ctx.accounts.pool_usdc.to_account_info().key;
        pool_account.distribution_authority = *ctx.accounts.distribution_authority.key;
        pool_account.nonce = nonce;
        pool_account.num_ido_tokens = num_ido_tokens;
        pool_account.max_usdc_tokens = max_usdc_tokens;
        pool_account.start_ido_ts = start_ido_ts;
        pool_account.end_ido_ts = end_ido_ts;
        pool_account.withdraw_melon_ts = withdraw_melon_ts;

        // Transfer Watermelon from creator to pool account.
        let cpi_accounts = Transfer {
            from: ctx.accounts.creator_watermelon.to_account_info(),
            to: ctx.accounts.pool_watermelon.to_account_info(),
            authority: ctx.accounts.payer.to_account_info(),
        };
        let cpi_program = ctx.accounts.token_program.to_account_info();
        let cpi_ctx = CpiContext::new(cpi_program, cpi_accounts);
        token::transfer(cpi_ctx, num_ido_tokens)?;

        Ok(())
    }

    pub fn modify_ido_time(
        ctx: Context<ModifyIdoTime>,
        start_ido_ts: i64,
        end_ido_ts: i64,
        withdraw_melon_ts: i64,
    ) -> Result<()> {
        if !(start_ido_ts < end_ido_ts && end_ido_ts < withdraw_melon_ts) {
            return Err(ErrorCode::SeqTimes.into());
        }
        if Pubkey::from_str(ALLOWED_DEPLOYER).unwrap() != *ctx.accounts.payer.to_account_info().key
        {
            return Err(ErrorCode::InvalidParam.into());
        }
        let pool_account = &mut ctx.accounts.pool_account;
        pool_account.start_ido_ts = start_ido_ts;
        pool_account.end_ido_ts = end_ido_ts;
        pool_account.withdraw_melon_ts = withdraw_melon_ts;
        Ok(())
    }

    pub fn modify_max_usdc_tokens(
        ctx: Context<ModifyMaxUsdcTokens>,
        max_usdc_tokens: u64,
    ) -> Result<()> {
        if max_usdc_tokens == 0 {
            return Err(ErrorCode::InvalidParam.into());
        }
        if Pubkey::from_str(ALLOWED_DEPLOYER).unwrap() != *ctx.accounts.payer.to_account_info().key
        {
            return Err(ErrorCode::InvalidParam.into());
        }
        let pool_account = &mut ctx.accounts.pool_account;
        pool_account.max_usdc_tokens = max_usdc_tokens;
        Ok(())
    }

    #[access_control(unrestricted_phase(&ctx.accounts.pool_account))]
    pub fn exchange_usdc_for_redeemable(
        ctx: Context<ExchangeUsdcForRedeemable>,
        amount: u64,
    ) -> Result<()> {
        if amount == 0 {
            return Err(ErrorCode::InvalidParam.into());
        }

        // Determine exchangeable usdc amount for user.
        let amount = cmp::min(
            amount,
            ctx.accounts.pool_account.max_usdc_tokens - ctx.accounts.pool_account.num_usdc_tokens,
        );

        // While token::transfer will check this, we prefer a verbose err msg.
        if ctx.accounts.user_usdc.amount < amount {
            return Err(ErrorCode::LowUsdc.into());
        }

        // Transfer user's USDC to pool USDC account.
        let cpi_accounts = Transfer {
            from: ctx.accounts.user_usdc.to_account_info(),
            to: ctx.accounts.pool_usdc.to_account_info(),
            authority: ctx.accounts.user_authority.to_account_info(),
        };
        let cpi_program = ctx.accounts.token_program.to_account_info();
        let cpi_ctx = CpiContext::new(cpi_program, cpi_accounts);
        token::transfer(cpi_ctx, amount)?;

        // Update USDC amounts on the pool account
        ctx.accounts.pool_account.num_usdc_tokens = (ctx.accounts.pool_account.num_usdc_tokens
            as u128)
            .checked_add(amount as u128)
            .unwrap()
            .try_into()
            .unwrap();

        // Mint Redeemable to user Redeemable account.
        let seeds = &[
            ctx.accounts.pool_account.watermelon_mint.as_ref(),
            &[ctx.accounts.pool_account.nonce],
        ];
        let signer = &[&seeds[..]];
        let cpi_accounts = MintTo {
            mint: ctx.accounts.redeemable_mint.to_account_info(),
            to: ctx.accounts.user_redeemable.to_account_info(),
            authority: ctx.accounts.pool_signer.to_account_info(),
        };
        let cpi_program = ctx.accounts.token_program.to_account_info();
        let cpi_ctx = CpiContext::new_with_signer(cpi_program, cpi_accounts, signer);
        token::mint_to(cpi_ctx, amount)?;

        Ok(())
    }

    #[access_control(unrestricted_phase(&ctx.accounts.pool_account))]
    pub fn exchange_redeemable_for_usdc(
        ctx: Context<ExchangeRedeemableForUsdc>,
        amount: u64,
    ) -> Result<()> {
        if amount == 0 {
            return Err(ErrorCode::InvalidParam.into());
        }

        // While token::burn will check this, we prefer a verbose err msg.
        if ctx.accounts.user_redeemable.amount < amount {
            return Err(ErrorCode::LowRedeemable.into());
        }

        // Burn the user's redeemable tokens.
        let cpi_accounts = Burn {
            mint: ctx.accounts.redeemable_mint.to_account_info(),
            to: ctx.accounts.user_redeemable.to_account_info(),
            authority: ctx.accounts.user_authority.to_account_info(),
        };
        let cpi_program = ctx.accounts.token_program.to_account_info();
        let cpi_ctx = CpiContext::new(cpi_program, cpi_accounts);
        token::burn(cpi_ctx, amount)?;

        // Transfer USDC from pool account to user.
        let seeds = &[
            ctx.accounts.pool_account.watermelon_mint.as_ref(),
            &[ctx.accounts.pool_account.nonce],
        ];
        let signer = &[&seeds[..]];
        let cpi_accounts = Transfer {
            from: ctx.accounts.pool_usdc.to_account_info(),
            to: ctx.accounts.user_usdc.to_account_info(),
            authority: ctx.accounts.pool_signer.to_account_info(),
        };
        let cpi_program = ctx.accounts.token_program.to_account_info();
        let cpi_ctx = CpiContext::new_with_signer(cpi_program, cpi_accounts, signer);
        token::transfer(cpi_ctx, amount)?;

        // Update USDC amounts on the pool account
        ctx.accounts.pool_account.num_usdc_tokens = (ctx.accounts.pool_account.num_usdc_tokens
            as u128)
            .checked_sub(amount as u128)
            .unwrap()
            .try_into()
            .unwrap();

        Ok(())
    }

    #[access_control(ido_over(&ctx.accounts.pool_account))]
    pub fn exchange_redeemable_for_watermelon(
        ctx: Context<ExchangeRedeemableForWatermelon>,
        amount: u64,
    ) -> Result<()> {
        if amount == 0 {
            return Err(ErrorCode::InvalidParam.into());
        }
        // While token::burn will check this, we prefer a verbose err msg.
        if ctx.accounts.user_redeemable.amount < amount {
            return Err(ErrorCode::LowRedeemable.into());
        }

        // Calculate watermelon tokens due.
        let watermelon_amount = cmp::min(
            ctx.accounts.pool_watermelon.amount,
            (amount as u128)
                .checked_mul(ctx.accounts.pool_watermelon.amount as u128)
                .unwrap()
                .checked_div(ctx.accounts.redeemable_mint.supply as u128)
                .unwrap()
                .try_into()
                .unwrap(),
        );

        // Burn the user's redeemable tokens.
        let cpi_accounts = Burn {
            mint: ctx.accounts.redeemable_mint.to_account_info(),
            to: ctx.accounts.user_redeemable.to_account_info(),
            authority: ctx.accounts.user_authority.to_account_info(),
        };
        let cpi_program = ctx.accounts.token_program.to_account_info();
        let cpi_ctx = CpiContext::new(cpi_program, cpi_accounts);
        token::burn(cpi_ctx, amount)?;

        // Transfer Watermelon from pool account to user.
        let seeds = &[
            ctx.accounts.pool_account.watermelon_mint.as_ref(),
            &[ctx.accounts.pool_account.nonce],
        ];
        let signer = &[&seeds[..]];
        let cpi_accounts = Transfer {
            from: ctx.accounts.pool_watermelon.to_account_info(),
            to: ctx.accounts.user_watermelon.to_account_info(),
            authority: ctx.accounts.pool_signer.to_account_info(),
        };
        let cpi_program = ctx.accounts.token_program.to_account_info();
        let cpi_ctx = CpiContext::new_with_signer(cpi_program, cpi_accounts, signer);
        token::transfer(cpi_ctx, watermelon_amount as u64)?;

        Ok(())
    }

    #[access_control(ido_over(&ctx.accounts.pool_account))]
    pub fn withdraw_pool_usdc(ctx: Context<WithdrawPoolUsdc>, amount: u64) -> Result<()> {
        if Pubkey::from_str(ALLOWED_DEPLOYER).unwrap() != *ctx.accounts.payer.to_account_info().key
        {
            return Err(ErrorCode::InvalidParam.into());
        }
        // Transfer total USDC from pool account to creator account.
        let seeds = &[
            ctx.accounts.pool_account.watermelon_mint.as_ref(),
            &[ctx.accounts.pool_account.nonce],
        ];
        let signer = &[&seeds[..]];
        let cpi_accounts = Transfer {
            from: ctx.accounts.pool_usdc.to_account_info(),
            to: ctx.accounts.creator_usdc.to_account_info(),
            authority: ctx.accounts.pool_signer.to_account_info(),
        };
        let cpi_program = ctx.accounts.token_program.to_account_info();
        let cpi_ctx = CpiContext::new_with_signer(cpi_program, cpi_accounts, signer);
        token::transfer(cpi_ctx, amount)?;

        Ok(())
    }
}

#[derive(Accounts)]
pub struct InitializePool<'info> {
    #[account(zero)]
    pub pool_account: Box<Account<'info, PoolAccount>>,
    pub pool_signer: AccountInfo<'info>,
    #[account(
        constraint = redeemable_mint.mint_authority == COption::Some(*pool_signer.key),
        constraint = redeemable_mint.supply == 0
    )]
    pub redeemable_mint: Box<Account<'info, Mint>>,
    #[account(constraint = usdc_mint.decimals == redeemable_mint.decimals)]
    pub usdc_mint: Box<Account<'info, Mint>>,
    #[account(constraint = pool_watermelon.mint == *watermelon_mint.to_account_info().key)]
    pub watermelon_mint: Box<Account<'info, Mint>>,
    #[account(mut, constraint = pool_watermelon.owner == *pool_signer.key)]
    pub pool_watermelon: Box<Account<'info, TokenAccount>>,
    #[account(constraint = pool_usdc.owner == *pool_signer.key)]
    pub pool_usdc: Box<Account<'info, TokenAccount>>,
    #[account(constraint =  watermelon_mint.mint_authority == COption::Some(*distribution_authority.key))]
    pub distribution_authority: Signer<'info>,
    pub payer: Signer<'info>,
    #[account(mut)]
    pub creator_watermelon: Box<Account<'info, TokenAccount>>,
    pub system_program: Program<'info, System>,
    pub token_program: Program<'info, Token>,
    pub rent: Sysvar<'info, Rent>,
}

impl<'info> InitializePool<'info> {
    fn accounts(ctx: &Context<InitializePool<'info>>, nonce: u8) -> Result<()> {
        let expected_signer = Pubkey::create_program_address(
            &[ctx.accounts.pool_watermelon.mint.as_ref(), &[nonce]],
            ctx.program_id,
        )
        .map_err(|_| ErrorCode::InvalidNonce)?;
        if ctx.accounts.pool_signer.key != &expected_signer {
            return Err(ErrorCode::InvalidNonce.into());
        }
        Ok(())
    }
}

#[derive(Accounts)]
pub struct ExchangeUsdcForRedeemable<'info> {
    #[account(
        has_one = redeemable_mint, 
        has_one = pool_usdc
    )]
    pub pool_account: Account<'info, PoolAccount>,
    #[account(seeds = [
        pool_account.watermelon_mint.as_ref()],
        bump=pool_account.nonce
    )]
    pool_signer: AccountInfo<'info>,
    #[account(
        mut,
        constraint = redeemable_mint.mint_authority == COption::Some(*pool_signer.key)
    )]
    pub redeemable_mint: Account<'info, Mint>,
    #[account(mut, constraint = pool_usdc.owner == *pool_signer.key)]
    pub pool_usdc: Account<'info, TokenAccount>,
    pub user_authority: Signer<'info>,
    #[account(mut, constraint = user_usdc.owner == *user_authority.key)]
    pub user_usdc: Account<'info, TokenAccount>,
    #[account(mut, constraint = user_redeemable.owner == *user_authority.key)]
    pub user_redeemable: Account<'info, TokenAccount>,
    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
pub struct ExchangeRedeemableForUsdc<'info> {
    #[account(has_one = redeemable_mint, has_one = pool_usdc)]
    pub pool_account: Account<'info, PoolAccount>,
    #[account(seeds = [
        pool_account.watermelon_mint.as_ref()],
        bump=pool_account.nonce
    )]
    pool_signer: AccountInfo<'info>,
    #[account(
        mut,
        constraint = redeemable_mint.mint_authority == COption::Some(*pool_signer.key)
    )]
    pub redeemable_mint: Account<'info, Mint>,
    #[account(mut, constraint = pool_usdc.owner == *pool_signer.key)]
    pub pool_usdc: Account<'info, TokenAccount>,
    pub user_authority: Signer<'info>,
    #[account(mut, constraint = user_usdc.owner == *user_authority.key)]
    pub user_usdc: Account<'info, TokenAccount>,
    #[account(mut, constraint = user_redeemable.owner == *user_authority.key)]
    pub user_redeemable: Account<'info, TokenAccount>,
    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
pub struct ExchangeRedeemableForWatermelon<'info> {
    #[account(has_one = redeemable_mint, has_one = pool_watermelon)]
    pub pool_account: Account<'info, PoolAccount>,
    #[account(seeds = [
        pool_account.watermelon_mint.as_ref()],
        bump=pool_account.nonce
    )]
    pool_signer: AccountInfo<'info>,
    #[account(
        mut,
        constraint = redeemable_mint.mint_authority == COption::Some(*pool_signer.key)
    )]
    pub redeemable_mint: Account<'info, Mint>,
    #[account(mut, constraint = pool_watermelon.owner == *pool_signer.key)]
    pub pool_watermelon: Account<'info, TokenAccount>,
    #[account(signer)]
    pub user_authority: AccountInfo<'info>,
    #[account(mut, constraint = user_watermelon.owner == *user_authority.key)]
    pub user_watermelon: Account<'info, TokenAccount>,
    #[account(mut, constraint = user_redeemable.owner == *user_authority.key)]
    pub user_redeemable: Account<'info, TokenAccount>,
    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
pub struct WithdrawPoolUsdc<'info> {
    #[account(has_one = pool_usdc, has_one = distribution_authority)]
    pub pool_account: Account<'info, PoolAccount>,
    #[account(seeds = [
        pool_account.watermelon_mint.as_ref()],
        bump=pool_account.nonce
    )]
    pub pool_signer: AccountInfo<'info>,
    #[account(mut, constraint = pool_usdc.owner == *pool_signer.key)]
    pub pool_usdc: Account<'info, TokenAccount>,
    pub distribution_authority: Signer<'info>,
    pub payer: Signer<'info>,
    #[account(mut)]
    pub creator_usdc: Account<'info, TokenAccount>,
    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
pub struct ModifyIdoTime<'info> {
    #[account(mut, has_one = distribution_authority)]
    pub pool_account: Account<'info, PoolAccount>,
    #[account(signer)]
    pub distribution_authority: AccountInfo<'info>,
    #[account(signer)]
    pub payer: AccountInfo<'info>,
}

#[derive(Accounts)]
pub struct ModifyMaxUsdcTokens<'info> {
    #[account(mut, has_one = distribution_authority)]
    pub pool_account: Account<'info, PoolAccount>,
    #[account(signer)]
    pub distribution_authority: AccountInfo<'info>,
    #[account(signer)]
    pub payer: AccountInfo<'info>,
}

#[account]
#[derive(Default)]
pub struct PoolAccount {
    pub redeemable_mint: Pubkey,
    pub pool_watermelon: Pubkey,
    pub watermelon_mint: Pubkey,
    pub pool_usdc: Pubkey,
    pub distribution_authority: Pubkey,
    pub nonce: u8,
    pub num_ido_tokens: u64,
    pub max_usdc_tokens: u64,
    pub num_usdc_tokens: u64,
    pub start_ido_ts: i64,
    pub end_ido_ts: i64,
    pub withdraw_melon_ts: i64,
}

#[error]
pub enum ErrorCode {
    #[msg("IDO must start in the future")]
    IdoFuture, //300, 0x12c
    #[msg("IDO times are non-sequential")]
    SeqTimes, //301, 0x12d
    #[msg("IDO has not started")]
    StartIdoTime, //302, 0x12e
    #[msg("Deposits period has ended")]
    EndDepositsTime, //303, 0x12f
    #[msg("IDO has ended")]
    EndIdoTime, //304, 0x130
    #[msg("IDO has not finished yet")]
    IdoNotOver, //305, 0x131
    #[msg("Insufficient USDC")]
    LowUsdc, //306, 0x132
    #[msg("Insufficient redeemable tokens")]
    LowRedeemable, //307, 0x133
    #[msg("USDC total and redeemable total don't match")]
    UsdcNotEqRedeem, //308, 0x134
    #[msg("Given nonce is invalid")]
    InvalidNonce, //309, 0x135
    #[msg("Invalid param")]
    InvalidParam, //310, 0x136
    #[msg("Exceed USDC")]
    ExceedUsdc, // 311, 0x137
}

// Access control modifiers.

// Asserts the IDO starts in the future.
fn future_start_time(start_ido_ts: i64) -> Result<()> {
    let now_ts = Clock::get().unwrap().unix_timestamp;

    if !(now_ts < start_ido_ts) {
        return Err(ErrorCode::IdoFuture.into());
    }
    Ok(())
}

// Asserts the IDO is in the first phase.
fn unrestricted_phase<'info>(
    pool_account: &Account<'info, PoolAccount>,
) -> Result<()> {
    let now_ts = Clock::get().unwrap().unix_timestamp;

    if !(pool_account.start_ido_ts < now_ts) {
        return Err(ErrorCode::StartIdoTime.into());
    } else if !(now_ts < pool_account.end_ido_ts) {
        return Err(ErrorCode::EndDepositsTime.into());
    } else if !(pool_account.num_usdc_tokens < pool_account.max_usdc_tokens) {
        return Err(ErrorCode::ExceedUsdc.into());
    }
    Ok(())
}

// Asserts the IDO sale period has ended, based on the current timestamp.
fn ido_over<'info>(
    pool_account: &Account<'info, PoolAccount>,
) -> Result<()> {
    let now_ts = Clock::get().unwrap().unix_timestamp;

    if pool_account.num_usdc_tokens >= pool_account.max_usdc_tokens {
        return Ok(());
    } else if !(pool_account.withdraw_melon_ts < now_ts) {
        return Err(ErrorCode::IdoNotOver.into());
    }
    Ok(())
}
